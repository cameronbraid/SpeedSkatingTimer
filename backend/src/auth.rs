//! NATS JWT Authentication Module
//!
//! Handles PIN code authentication and JWT token generation for NATS clients.

use std::fs;
use std::path::Path;

use async_nats::service::ServiceExt;
use async_nats::Client;
use color_eyre::Result;
use futures::StreamExt;
use nats_jwt::{KeyPair, Token};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::types::AuthResponse;

/// NATS account permissions
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct NatsPermissions {
    /// Allowed publish subjects (wildcards supported)
    pub publish: Vec<String>,
    /// Allowed subscribe subjects (wildcards supported)
    pub subscribe: Vec<String>,
}

/// PIN code configuration entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinConfig {
    /// PIN code (4 or 6 digits)
    pub pin: String,
    /// NATS permissions for this PIN
    pub permissions: NatsPermissions,
}

/// Configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// PIN code to permissions mapping
    pub pins: Vec<PinConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            pins: vec![
                // Default admin PIN with full access
                PinConfig {
                    pin: "1234".to_string(),
                    permissions: NatsPermissions {
                        publish: vec![">".to_string()],
                        subscribe: vec![">".to_string()],
                    },
                },
            ],
        }
    }
}

/// Load authentication configuration from file
pub fn load_auth_config(config_path: &Path) -> Result<AuthConfig> {
    if !config_path.exists() {
        warn!("Auth config file not found at {:?}, creating default", config_path);
        let default_config = AuthConfig::default();
        let config_str = serde_yaml::to_string(&default_config)?;
        fs::write(config_path, config_str)?;
        info!("Created default auth config at {:?}", config_path);
        return Ok(default_config);
    }

    let config_str = fs::read_to_string(config_path)?;
    let config: AuthConfig = serde_yaml::from_str(&config_str)?;
    info!("Loaded auth config from {:?} with {} PIN entries", config_path, config.pins.len());
    Ok(config)
}


/// Generate a NATS JWT token for a user with given permissions
pub fn generate_nats_jwt(
    account_key: &KeyPair,
    user_pub_key: &str,
    permissions: &NatsPermissions,
) -> Result<String> {
    
    // Start building the user token
    let mut token_builder = Token::new_user(account_key.public_key(), user_pub_key)
        .name(user_pub_key)
        .max_data(-1)
        .max_payload(-1)
        .max_subscriptions(-1)
        .bearer_token(true)
        .expires((chrono::Utc::now() + chrono::Duration::hours(4)).timestamp());
    
    // Set publish permissions
    if !permissions.publish.is_empty() {
        if permissions.publish.contains(&">".to_string()) {
            // Allow all
            token_builder = token_builder.allow_publish(">");
        } else {
            // Allow specific subjects
            for subject in &permissions.publish {
                token_builder = token_builder.allow_publish(subject);
            }
        }
    }
    
    // Always include "_INBOX.>" in subscribe permissions for request-reply pattern
    let mut subscribe_perms = permissions.subscribe.clone();
    if !subscribe_perms.contains(&"_INBOX.>".to_string()) {
        subscribe_perms.push("_INBOX.>".to_string());
    }
    
    // Set subscribe permissions
    if subscribe_perms.contains(&">".to_string()) {
        // Allow all
        token_builder = token_builder.allow_subscribe(">");
    } else {
        // Allow specific subjects
        for subject in &subscribe_perms {
            token_builder = token_builder.allow_subscribe(subject);
        }
    }
    
    // Sign the token with the account key
    let token = token_builder.sign(account_key);
    
    Ok(token)
}

/// Authenticate a PIN code and return JWT token
pub fn authenticate_pin(
    pin: &str,
    config: &AuthConfig,
    account_key: &KeyPair,
    user_public_key: &str,
) -> Result<Option<String>> {
    // Validate PIN format (4 or 6 digits)
    if !pin.chars().all(|c| c.is_ascii_digit()) || (pin.len() != 4) {
        return Ok(None);
    }

    // Find matching PIN in config
    let pin_config = config.pins.iter().find(|p| p.pin == pin);
    
    match pin_config {
        Some(config) => {
            // Use the frontend user's public key as the subject
            // This ensures the JWT matches the NKey seed used by the frontend
            let jwt = generate_nats_jwt(
                account_key,
                user_public_key,
                &config.permissions,
            )?;
            info!("Authenticated PIN {} for user {}", pin, user_public_key);
            Ok(Some(jwt))
        }
        None => {
            warn!("Invalid PIN code attempted: {}", pin);
            Ok(None)
        }
    }
}

/// Run the NATS authentication service
pub async fn run_auth_service(
    nats: Client,
    config: AuthConfig,
    account_key: KeyPair,
    user_public_key: String,
) -> Result<()> {
    // Create NATS service
    let service = nats
        .service_builder()
        .description("NATS authentication service")
        .start("system", "1.0.0")
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Failed to create NATS service: {}", e))?;

    info!("NATS auth service started");

    // Create service group
    let auth_group = service.group("system.v1");

    // Create auth endpoint
    let mut auth_endpoint = auth_group
        .endpoint("auth")
        .await
        .map_err(|e| color_eyre::eyre::eyre!("Failed to create auth endpoint: {}", e))?;

    info!("NATS auth service listening on 'system.v1.auth' endpoint");

    while let Some(request) = auth_endpoint.next().await {
        let pin = match String::from_utf8(request.message.payload.to_vec()) {
            Ok(p) => p.trim().to_string(),
            Err(e) => {
                error!("Invalid PIN format in auth request: {}", e);
                let auth_response = AuthResponse::error(
                    format!("Invalid PIN format: {}", e),
                    "INVALID_FORMAT",
                );
                let response = serde_json::to_vec(&auth_response)
                    .unwrap_or_else(|_| b"{\"type\":\"error\",\"error\":\"Invalid PIN format\",\"code\":\"INVALID_FORMAT\"}".to_vec());
                let _ = request.respond(Ok(response.into())).await;
                continue;
            }
        };

        let auth_response = match authenticate_pin(&pin, &config, &account_key, &user_public_key) {
            Ok(Some(jwt)) => {
                info!("Authentication successful for PIN: {}", pin);
                AuthResponse::jwt(jwt)
            }
            Ok(None) => {
                warn!("Authentication failed for PIN: {}", pin);
                AuthResponse::error("Invalid PIN code", "INVALID_PIN")
            }
            Err(e) => {
                error!("Error during authentication: {}", e);
                AuthResponse::error(format!("{}", e), "AUTH_ERROR")
            }
        };

        let response = serde_json::to_vec(&auth_response)
            .unwrap_or_else(|_| {
                // Fallback error response if serialization fails
                serde_json::to_vec(&AuthResponse::error(
                    "Failed to serialize response",
                    "SERIALIZATION_ERROR",
                ))
                .unwrap_or_else(|_| b"{\"type\":\"error\",\"error\":\"Internal error\",\"code\":\"INTERNAL_ERROR\"}".to_vec())
            });

        if let Err(e) = request.respond(Ok(response.into())).await {
            error!("Failed to send auth response: {}", e);
        }
    }

    Ok(())
}



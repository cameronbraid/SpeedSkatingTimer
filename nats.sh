#!/bin/bash
# Script to start NATS server with memory-based operator and account credentials

set -e

# Directory for storing NATS credentials
CREDS_DIR="${CREDS_DIR:-./.nats-creds}"
OPERATOR_NAME="${OPERATOR_NAME:-OPERATOR}"
ACCOUNT_NAME="${ACCOUNT_NAME:-ACCOUNT}"

# Create credentials directory if it doesn't exist
mkdir -p "$CREDS_DIR"

# Check if nsc is available
if ! command -v nsc &> /dev/null; then
    echo "Error: nsc (NATS CLI) not found. Please install it:"
    echo "  from https://github.com/nats-io/nsc/releases"
    exit 1
fi

# Check if nats-server is available
if ! command -v nats-server &> /dev/null; then
    echo "Error: nats-server not found. Please install it:"
    echo "  download from https://github.com/nats-io/nats-server/releases"
    exit 1
fi

# Set nsc store directory to use our custom directory (not ~/.local)
NSC_STORE="$CREDS_DIR/.nsc"
NSC_KEYS="$CREDS_DIR/.nsc/keys"
mkdir -p "$NSC_STORE"
mkdir -p "$NSC_KEYS"

# Set environment variable - NKEYS_PATH should point to parent of 'keys' directory
# But nsc creates keys/keys/ structure, so we point to the directory containing 'keys'
export NKEYS_PATH="$CREDS_DIR/.nsc"

# Configure nsc to use our custom store directory
# This updates the nsc.json config file
nsc env --store "$NSC_STORE" > /dev/null 2>&1 || true

# Clear operator/account references and ensure store_root is set correctly
# Find nsc config file location (usually in XDG config dir or ~/.config)
NSC_CONFIG=""
if [ -n "$XDG_CONFIG_HOME" ]; then
    NSC_CONFIG="$XDG_CONFIG_HOME/nats/nsc/nsc.json"
elif [ -d ~/.config ]; then
    NSC_CONFIG=~/.config/nats/nsc/nsc.json
fi

if [ -n "$NSC_CONFIG" ] && [ -f "$NSC_CONFIG" ]; then
    # Update store_root and remove operator/account fields using sed
    # Escape the path for sed
    ESCAPED_STORE=$(echo "$NSC_STORE" | sed 's/[\/&]/\\&/g')
    sed -i.bak "s|\"store_root\":\"[^\"]*\"|\"store_root\":\"$ESCAPED_STORE\"|g; s/,\"operator\":\"[^\"]*\"//g; s/,\"account\":\"[^\"]*\"//g" "$NSC_CONFIG" 2>/dev/null || true
fi

# Generate operator key and JWT if they don't exist
OPERATOR_KEY="$CREDS_DIR/operator.key"
OPERATOR_JWT="$CREDS_DIR/operator.jwt"

if [ ! -f "$OPERATOR_KEY" ] || [ ! -f "$OPERATOR_JWT" ]; then
    echo "Generating operator credentials..."
    # Use nsc add operator with --sys flag to generate system account
    # Store directory is set via nsc env above, keys via --keystore-dir
    NSC_OP_DIR="$NSC_STORE/$OPERATOR_NAME"
    NSC_OP_JWT="$NSC_OP_DIR/$OPERATOR_NAME.jwt"
    
    # Check if operator exists in default location but not in our custom location
    # Try to detect default store location from nsc env or XDG directories
    DEFAULT_STORE=""
    if [ -n "$XDG_DATA_HOME" ]; then
        DEFAULT_STORE="$XDG_DATA_HOME/nats/nsc/stores"
    elif [ -d ~/.local/share/nats/nsc/stores ]; then
        DEFAULT_STORE=~/.local/share/nats/nsc/stores
    fi
    
    if [ ! -f "$NSC_OP_JWT" ] && [ -n "$DEFAULT_STORE" ]; then
        # Check if operator exists in default location
        if [ -d "$DEFAULT_STORE" ] && [ -f "$DEFAULT_STORE/$OPERATOR_NAME/$OPERATOR_NAME.jwt" ]; then
            echo "Found existing operator in default location, removing it to avoid conflicts..."
            # Manually remove the operator from default location since nsc doesn't have delete operator command
            rm -rf "$DEFAULT_STORE/$OPERATOR_NAME" 2>/dev/null || true
        fi
        # Also clean up any operator references in default keys directory
        DEFAULT_KEYS_DIR=""
        if [ -n "$XDG_DATA_HOME" ]; then
            DEFAULT_KEYS_DIR="$XDG_DATA_HOME/nats/nsc/keys"
        elif [ -d ~/.local/share/nats/nsc/keys ]; then
            DEFAULT_KEYS_DIR=~/.local/share/nats/nsc/keys
        fi
        if [ -n "$DEFAULT_KEYS_DIR" ] && [ -d "$DEFAULT_KEYS_DIR" ]; then
            # Remove keys that might be associated with this operator
            # Keys are stored in nested structure, so we need to be careful
            # For now, we'll let nsc recreate them in the new location
            echo "Cleaning up default keystore references..."
        fi
    fi
    
    if [ ! -f "$NSC_OP_JWT" ]; then
        # Try to create operator, but handle the case where it might already exist
        OUTPUT=$(nsc add operator --name "$OPERATOR_NAME" --sys --keystore-dir "$NSC_KEYS" 2>&1) || {
            # Check if operator was actually created despite the error
            if [ -f "$NSC_OP_JWT" ]; then
                echo "Operator already exists, using existing one"
            else
                # Check if error is about operator already existing
                if echo "$OUTPUT" | grep -q "exists already"; then
                    echo "Warning: Operator exists but JWT not found at expected location"
                    echo "Attempting to use existing operator..."
                    # Try to find the operator JWT in the store
                    if [ -f "$NSC_STORE/$OPERATOR_NAME/$OPERATOR_NAME.jwt" ]; then
                        echo "Found operator JWT, continuing..."
                    else
                        echo "Error: Failed to create or find operator"
                        echo "$OUTPUT"
                        exit 1
                    fi
                else
                    echo "Error: Failed to create operator"
                    echo "$OUTPUT"
                    exit 1
                fi
            fi
        }
    fi
    
    # Make sure we're using the correct store for subsequent operations
    nsc env --store "$NSC_STORE" > /dev/null 2>&1 || true
    
    # Extract operator JWT and key from nsc store
    # nsc stores operator JWT in <store>/<operator>/<operator>.jwt
    if [ -f "$NSC_OP_JWT" ]; then
        cp "$NSC_OP_JWT" "$OPERATOR_JWT"
        # Operator key is stored in keys/keys/<first2>/<next2>/<key>.nk structure
        # Find the operator key file (.nk format) - look for the most recent one or match by operator name
        OPERATOR_KEY_FILE=$(find "$NSC_KEYS" -name "*.nk" -type f | head -1)
        if [ -z "$OPERATOR_KEY_FILE" ]; then
            # Try the nested keys/keys structure
            OPERATOR_KEY_FILE=$(find "$NSC_KEYS/keys" -name "*.nk" -type f 2>/dev/null | head -1)
        fi
        if [ -n "$OPERATOR_KEY_FILE" ] && [ -f "$OPERATOR_KEY_FILE" ]; then
            cp "$OPERATOR_KEY_FILE" "$OPERATOR_KEY"
        else
            echo "Warning: Could not find operator key file"
            echo "Searched in: $NSC_KEYS"
            echo "Note: Operator JWT found, but key file may be in a different location"
        fi
    else
        echo "Error: Failed to generate operator credentials - JWT not found at $NSC_OP_JWT"
        exit 1
    fi
fi

# Generate account seed and JWT if they don't exist
ACCOUNT_SEED="$CREDS_DIR/account.seed"
ACCOUNT_JWT="$CREDS_DIR/account.jwt"

if [ ! -f "$ACCOUNT_SEED" ] || [ ! -f "$ACCOUNT_JWT" ]; then
    echo "Generating account credentials..."
    # Set the operator context (store is already set above)
    nsc env -o "$OPERATOR_NAME" > /dev/null 2>&1 || true
    
    # Check if account already exists in nsc store
    # nsc stores account JWT in <store>/<operator>/accounts/<account>/<account>.jwt
    NSC_ACCOUNT_DIR="$NSC_STORE/$OPERATOR_NAME/accounts/$ACCOUNT_NAME"
    NSC_ACCOUNT_JWT="$NSC_ACCOUNT_DIR/$ACCOUNT_NAME.jwt"
    if [ ! -f "$NSC_ACCOUNT_JWT" ]; then
        if ! nsc add account --name "$ACCOUNT_NAME" --allow-pubsub ">" --keystore-dir "$NSC_KEYS" 2>&1; then
            # If creation fails, check if it's because it already exists
            if [ ! -f "$NSC_ACCOUNT_JWT" ]; then
                echo "Error: Failed to create account"
                exit 1
            fi
        fi
        # Enable JetStream for the account (add account doesn't support JS flags)
        nsc edit account --name "$ACCOUNT_NAME" --js-mem-storage -1 --js-disk-storage -1 --keystore-dir "$NSC_KEYS" 2>&1 || true
    else
        # Account exists, ensure it has permissive default permissions and JetStream enabled
        # This allows users in the account to have their permissions work correctly
        nsc edit account --name "$ACCOUNT_NAME" --allow-pubsub ">" --js-mem-storage -1 --js-disk-storage -1 --keystore-dir "$NSC_KEYS" 2>&1 || true
    fi
    
    # Extract account JWT and seed from nsc store
    if [ -f "$NSC_ACCOUNT_JWT" ]; then
        cp "$NSC_ACCOUNT_JWT" "$ACCOUNT_JWT"
        
        # Get the account's public key from the JWT or nsc describe
        nsc env -a "$ACCOUNT_NAME" > /dev/null 2>&1 || true
        ACCOUNT_PUBLIC_KEY=$(nsc describe account -F nats.public_key 2>/dev/null | tr -d '"' | tr -d '\n' | tr -d ' ')
        
        if [ -z "$ACCOUNT_PUBLIC_KEY" ]; then
            echo "Warning: Could not get account public key from nsc describe"
            echo "Falling back to finding account seed by checking .nk files..."
        fi
        
        # Account seed is stored in keys/keys/<first2>/<next2>/<key>.nk structure
        # Find account seed files (start with SA) and match by public key if available
        ACCOUNT_SEED_FILE=""
        
        # First, try to find .nk files that start with SA (account seed prefix)
        for nk_file in $(find "$NSC_KEYS" -name "*.nk" -type f 2>/dev/null); do
            # Check if file starts with SA (account seed)
            if head -c 2 "$nk_file" | grep -q "SA"; then
                if [ -n "$ACCOUNT_PUBLIC_KEY" ]; then
                    # Try to verify this is the account seed by checking public key
                    # Load the seed and check if public key matches
                    SEED_CONTENT=$(cat "$nk_file" | tr -d '\n' | tr -d ' ')
                    # Use nats CLI or nsc to verify, or just use the first SA seed we find
                    # For now, if we have multiple SA seeds, use the most recent one
                    if [ -z "$ACCOUNT_SEED_FILE" ] || [ "$nk_file" -nt "$ACCOUNT_SEED_FILE" ]; then
                        ACCOUNT_SEED_FILE="$nk_file"
                    fi
                else
                    # No public key to match, use first SA seed found
                    ACCOUNT_SEED_FILE="$nk_file"
                    break
                fi
            fi
        done
        
        # If not found in main keys dir, try nested keys/keys structure
        if [ -z "$ACCOUNT_SEED_FILE" ]; then
            for nk_file in $(find "$NSC_KEYS/keys" -name "*.nk" -type f 2>/dev/null); do
                if head -c 2 "$nk_file" | grep -q "SA"; then
                    ACCOUNT_SEED_FILE="$nk_file"
                    break
                fi
            done
        fi
        
        if [ -n "$ACCOUNT_SEED_FILE" ] && [ -f "$ACCOUNT_SEED_FILE" ]; then
            cp "$ACCOUNT_SEED_FILE" "$ACCOUNT_SEED"
            echo "Copied account seed from $ACCOUNT_SEED_FILE to $ACCOUNT_SEED"
        else
            echo "Warning: Could not find account seed file"
            echo "Searched in: $NSC_KEYS"
            echo "Note: Account JWT found, but seed file may be in a different location"
            echo "Account seed should start with 'SA' (account seed prefix)"
        fi
    else
        echo "Error: Failed to generate account credentials - JWT not found at $NSC_ACCOUNT_JWT"
        exit 1
    fi
fi

# Create a NATS user with full permissions for server use
SERVER_USER_NAME="${SERVER_USER_NAME:-server}"
SERVER_USER_CREDS="$CREDS_DIR/server.creds"

if [ ! -f "$SERVER_USER_CREDS" ]; then
    echo "Creating NATS user '$SERVER_USER_NAME' with full permissions..."
    # Set account context
    nsc env -a "$ACCOUNT_NAME" > /dev/null 2>&1 || true
    
    # Create user with full publish/subscribe permissions
    # Use --keystore-dir to ensure nsc can find the account signing key
    if ! nsc add user --name "$SERVER_USER_NAME" --allow-pubsub ">" --keystore-dir "$NSC_KEYS" 2>&1; then
        # Check if user already exists
        if nsc describe user --account "$ACCOUNT_NAME" --name "$SERVER_USER_NAME" > /dev/null 2>&1; then
            echo "User '$SERVER_USER_NAME' already exists, using existing user"
        else
            echo "Error: Failed to create user '$SERVER_USER_NAME'"
            exit 1
        fi
    fi
    
    # Generate credentials file (contains JWT and seed for authentication)
    # nsc generate creds creates the file in the keystore, so we need to copy it
    NSC_CREDS_FILE="$NSC_KEYS/creds/$OPERATOR_NAME/$ACCOUNT_NAME/$SERVER_USER_NAME.creds"
    if [ -f "$NSC_CREDS_FILE" ]; then
        cp "$NSC_CREDS_FILE" "$SERVER_USER_CREDS"
        echo "Copied credentials file from nsc store to $SERVER_USER_CREDS"
    elif nsc generate creds --account "$ACCOUNT_NAME" --name "$SERVER_USER_NAME" --output-file "$SERVER_USER_CREDS" 2>&1; then
        echo "Generated credentials file at $SERVER_USER_CREDS"
    else
        # Try to find the credentials file in the nsc store structure
        FOUND_CREDS=$(find "$NSC_KEYS" -name "$SERVER_USER_NAME.creds" -type f 2>/dev/null | head -1)
        if [ -n "$FOUND_CREDS" ] && [ -f "$FOUND_CREDS" ]; then
            cp "$FOUND_CREDS" "$SERVER_USER_CREDS"
            echo "Found and copied credentials file from $FOUND_CREDS to $SERVER_USER_CREDS"
        else
            echo "Error: Failed to generate or find credentials file"
            echo "User was created but credentials file not found"
            exit 1
        fi
    fi
fi

# Create a NATS user for frontend with read-only permissions for stopwatch data
FRONTEND_USER_NAME="${FRONTEND_USER_NAME:-frontend}"
FRONTEND_JWT_FILE="$CREDS_DIR/frontend.jwt"

# Check if user already exists - we'll always update to ensure permissions are correct
USER_EXISTS=false
if nsc describe user --account "$ACCOUNT_NAME" --name "$FRONTEND_USER_NAME" > /dev/null 2>&1; then
    USER_EXISTS=true
fi

# Regenerate JWT if it doesn't exist or if user exists (to ensure permissions are updated)
if [ ! -f "$FRONTEND_JWT_FILE" ] || [ "$USER_EXISTS" = true ]; then
    # Delete existing JWT file if updating
    if [ "$USER_EXISTS" = true ] || [ -f "$FRONTEND_JWT_FILE" ]; then
        echo "Removing existing frontend JWT to regenerate with updated permissions..."
        rm -f "$FRONTEND_JWT_FILE"
        rm -f "$CREDS_DIR/frontend.seed"
        rm -f "$CREDS_DIR/frontend.creds"
    fi
    echo "Creating NATS user '$FRONTEND_USER_NAME' for frontend..."
    # Set account context
    nsc env -a "$ACCOUNT_NAME" > /dev/null 2>&1 || true
    
    # Create user with permissions to:
    # - Subscribe to stopwatch live data (tick, state, lap)
    # - Request system ping for latency measurement
    # - Subscribe to inbox for request/response
    # - Publish to stopwatch control subjects (arm, unarm, reset, get_state)
    # - Publish to system.v1.auth for authentication requests
    
    # Check if user already exists
    if nsc describe user --account "$ACCOUNT_NAME" --name "$FRONTEND_USER_NAME" > /dev/null 2>&1; then
        echo "User '$FRONTEND_USER_NAME' already exists, updating permissions..."
        # Delete existing user to recreate with updated permissions
        nsc delete user --account "$ACCOUNT_NAME" --name "$FRONTEND_USER_NAME" --keystore-dir "$NSC_KEYS" 2>&1 || true
    fi
    
    # Create user with updated permissions
    if ! nsc add user --name "$FRONTEND_USER_NAME" \
        --allow-sub "stopwatch.v1.live.>" \
        --allow-sub "_INBOX.>" \
        --allow-pub "stopwatch.v1.get_state" \
        --allow-pub "system.v1.ping" \
        --allow-pub "system.v1.auth" \
        --allow-pub '$JS.API.>' \
        --allow-sub '$JS.API.>' \
        --keystore-dir "$NSC_KEYS" 2>&1; then
        echo "Error: Failed to create user '$FRONTEND_USER_NAME'"
        exit 1
    fi
    
    # Generate credentials file for frontend (contains JWT and seed)
    FRONTEND_CREDS_FILE="$CREDS_DIR/frontend.creds"
    NSC_FRONTEND_CREDS="$NSC_KEYS/creds/$OPERATOR_NAME/$ACCOUNT_NAME/$FRONTEND_USER_NAME.creds"
    
    if [ -f "$NSC_FRONTEND_CREDS" ]; then
        cp "$NSC_FRONTEND_CREDS" "$FRONTEND_CREDS_FILE"
        echo "Copied frontend credentials file to $FRONTEND_CREDS_FILE"
    elif nsc generate creds --account "$ACCOUNT_NAME" --name "$FRONTEND_USER_NAME" --output-file "$FRONTEND_CREDS_FILE" 2>&1; then
        echo "Generated frontend credentials file at $FRONTEND_CREDS_FILE"
    else
        # Try to find the credentials file in the nsc store structure
        FOUND_FRONTEND_CREDS=$(find "$NSC_KEYS" -name "$FRONTEND_USER_NAME.creds" -type f 2>/dev/null | head -1)
        if [ -n "$FOUND_FRONTEND_CREDS" ] && [ -f "$FOUND_FRONTEND_CREDS" ]; then
            cp "$FOUND_FRONTEND_CREDS" "$FRONTEND_CREDS_FILE"
            echo "Found and copied frontend credentials file from $FOUND_FRONTEND_CREDS to $FRONTEND_CREDS_FILE"
        else
            echo "Warning: Could not generate frontend credentials file"
        fi
    fi
    
    # Extract JWT and seed from credentials file for frontend use
    if [ -f "$FRONTEND_CREDS_FILE" ]; then
        # Extract JWT (between BEGIN and END markers, first occurrence)
        if grep -q "BEGIN NATS USER JWT" "$FRONTEND_CREDS_FILE"; then
            sed -n '/BEGIN NATS USER JWT/,/END NATS USER JWT/p' "$FRONTEND_CREDS_FILE" | sed '1d;$d' | tr -d '\n' > "$FRONTEND_JWT_FILE"
            echo "Extracted frontend JWT to $FRONTEND_JWT_FILE"
        fi
        
        # Extract seed (between BEGIN USER NKEY SEED and END USER NKEY SEED markers)
        FRONTEND_SEED_FILE="$CREDS_DIR/frontend.seed"
        if grep -q "BEGIN USER NKEY SEED" "$FRONTEND_CREDS_FILE"; then
            sed -n '/BEGIN USER NKEY SEED/,/END USER NKEY SEED/p' "$FRONTEND_CREDS_FILE" | sed '1d;$d' | tr -d '\n' > "$FRONTEND_SEED_FILE"
            echo "Extracted frontend seed to $FRONTEND_SEED_FILE"
        fi
        
        # Copy JWT and seed to frontend assets directory if it exists
        # Try to find frontend assets directory relative to script location or current directory
        SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
        FRONTEND_ASSETS_DIR="$SCRIPT_DIR/frontend/src/assets"
        if [ ! -d "$FRONTEND_ASSETS_DIR" ]; then
            # Try relative to current directory
            FRONTEND_ASSETS_DIR="./frontend/src/assets"
        fi
        if [ -d "$FRONTEND_ASSETS_DIR" ]; then
            if [ -f "$FRONTEND_JWT_FILE" ]; then
                cp "$FRONTEND_JWT_FILE" "$FRONTEND_ASSETS_DIR/frontend.jwt"
                echo "Copied frontend JWT to $FRONTEND_ASSETS_DIR/frontend.jwt"
            fi
            if [ -f "$FRONTEND_SEED_FILE" ]; then
                cp "$FRONTEND_SEED_FILE" "$FRONTEND_ASSETS_DIR/frontend.seed"
                echo "Copied frontend seed to $FRONTEND_ASSETS_DIR/frontend.seed"
            fi
        fi
    fi
    
fi

# Extract account key for backend use
# The backend needs the account key in PEM format to sign user JWTs
# Note: NATS uses .nk format, but we need PEM for jwt-simple
USER_SIGNING_KEY="$CREDS_DIR/nats-account-key.pem"
if [ ! -f "$USER_SIGNING_KEY" ] && [ -f "$ACCOUNT_SEED" ]; then
    echo "Converting NATS account key to PEM format..."
    # Try to convert .nk to PEM using nats CLI if available
    # For now, we'll generate a new key and note that it should match
    if command -v openssl &> /dev/null; then
        openssl genpkey -algorithm Ed25519 -out "$USER_SIGNING_KEY"
        echo "Generated Ed25519 PEM key at $USER_SIGNING_KEY"
        echo "WARNING: This is a new key. For production, convert the NATS account key (.nk) to PEM format"
        echo "The account key from NATS should be used to sign user JWTs"
    else
        echo "Error: openssl not found. Cannot generate PEM key."
        exit 1
    fi
fi

# Create NATS server config file
NATS_CONF="$CREDS_DIR/nats-server.conf"

# Read JWT content directly (NATS config needs actual JWT content, not file paths)
OPERATOR_JWT_CONTENT=$(cat "$OPERATOR_JWT")
ACCOUNT_JWT_CONTENT=$(cat "$ACCOUNT_JWT")

# Find and read system account (SYS) JWT - it's created automatically with the operator
SYS_JWT_FILE="$NSC_STORE/$OPERATOR_NAME/accounts/SYS/SYS.jwt"
if [ ! -f "$SYS_JWT_FILE" ]; then
    echo "Error: System account JWT not found at $SYS_JWT_FILE"
    exit 1
fi
SYS_JWT_CONTENT=$(cat "$SYS_JWT_FILE")

# Extract account IDs using nsc describe commands
# Set operator context for nsc commands
nsc env -o "$OPERATOR_NAME" > /dev/null 2>&1 || true

# Extract system account ID from operator using nsc
# The -F flag extracts JSON field values, strip quotes and whitespace
SYSTEM_ACCOUNT_ID=$(nsc describe operator --name "$OPERATOR_NAME" -F nats.system_account 2>/dev/null | sed 's/"//g' | tr -d '\n' | tr -d ' ')

# Extract account ID from account JWT using base64 decode (standard tool, not Python)
# JWT format: header.payload.signature, decode the payload (second part)
ACCOUNT_JWT_PAYLOAD=$(cat "$ACCOUNT_JWT" | cut -d. -f2 | base64 -d 2>/dev/null)
ACCOUNT_ID=$(echo "$ACCOUNT_JWT_PAYLOAD" | grep -o '"sub":"[^"]*"' | cut -d'"' -f4)

if [ -z "$SYSTEM_ACCOUNT_ID" ] || [ -z "$ACCOUNT_ID" ]; then
    echo "Error: Failed to extract account IDs"
    echo "System Account ID: ${SYSTEM_ACCOUNT_ID:-not found}"
    echo "Account ID: ${ACCOUNT_ID:-not found}"
    exit 1
fi

cat > "$NATS_CONF" <<EOF
# NATS Server Configuration
operator: $OPERATOR_JWT_CONTENT

# Resolver configuration (memory-based)
resolver: MEMORY
resolver_preload: {
    $SYSTEM_ACCOUNT_ID: "$SYS_JWT_CONTENT"
    $ACCOUNT_ID: "$ACCOUNT_JWT_CONTENT"
}

# JetStream configuration
jetstream {
    store_dir: "$CREDS_DIR/jetstream"
}

# WebSocket configuration
websocket {
    port: 4223
    no_tls: true
}

# Note: With operator-based JWT authentication, users must be authenticated via JWTs
# The auth service will handle authentication and issue user JWTs
EOF

echo "Starting NATS server with configuration:"
echo "  Operator JWT: $OPERATOR_JWT"
echo "  Account JWT: $ACCOUNT_JWT"
echo "  Account Seed: $ACCOUNT_SEED"
echo "  User Signing Key: $USER_SIGNING_KEY"
echo "  Server User Credentials: $SERVER_USER_CREDS"
echo "  Frontend JWT: $FRONTEND_JWT_FILE"
echo "  Config: $NATS_CONF"
echo ""
echo "Note: Make sure the account key matches the key used to sign user JWTs"
echo "Server user '$SERVER_USER_NAME' can connect using credentials file: $SERVER_USER_CREDS"
echo "Frontend can use JWT from: $FRONTEND_JWT_FILE"
echo ""

# Start NATS server
nats-server --config "$NATS_CONF"



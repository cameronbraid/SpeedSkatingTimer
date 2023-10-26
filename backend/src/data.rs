#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(tag = "type")]
pub enum DataMessage {
    #[serde(rename = "reset")]
    Reset(ResetMessage),

    #[serde(rename = "timestamp")]
    Timestamp(TimestampMessage),

    #[serde(rename = "setup")]
    Setup(SetupMessage),

    #[serde(rename = "subscribe-setup")]
    SubscribeSetup(SubscribeSetup),

    #[serde(rename = "unsubscribe-setup")]
    UnSubscribeSetup(UnSubscribeSetup),
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TimestampMessage {
    pub timestamp: u64,
    pub duration: Option<u64>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct SubscribeSetup {}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct UnSubscribeSetup {}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct SetupMessage {
    pub connected: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct ResetMessage {
    pub id: Option<String>,
}

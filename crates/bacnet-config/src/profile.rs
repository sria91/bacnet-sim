/// Device profile templates.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceProfile {
    pub name: String,
    pub description: String,
}

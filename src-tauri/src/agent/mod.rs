use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    Codex,
    Agy,
    Aider,
}

impl AgentType {
    pub fn binary(&self) -> &'static str {
        match self {
            AgentType::Claude => "claude",
            AgentType::Codex  => "codex",
            AgentType::Agy    => "agy",
            AgentType::Aider  => "robolaunch-aider",
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentType::Claude => "claude",
            AgentType::Codex  => "codex",
            AgentType::Agy    => "agy",
            AgentType::Aider  => "aider",
        }
    }
}

impl std::str::FromStr for AgentType {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(AgentType::Claude),
            "codex"  => Ok(AgentType::Codex),
            "agy"    => Ok(AgentType::Agy),
            "aider"  => Ok(AgentType::Aider),
            other    => anyhow::bail!("Unknown agent type: {other}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub id:         String,
    pub name:       String,
    pub agent_type: String,
    pub status:     AgentStatus,
    pub tab_label:  String,
    #[serde(skip_serializing)]
    pub mcp_token:  String,
}

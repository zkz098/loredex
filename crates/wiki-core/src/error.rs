use thiserror::Error;

#[derive(Debug, Error)]
pub enum WikiError {
    #[error("failed to parse frontmatter in {0}: {1}")]
    FrontmatterParse(String, String),
    #[error("missing frontmatter delimiter in {0}")]
    MissingFrontmatter(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("git error: {0}")]
    Git(String),
    #[error("page not found: {0}")]
    PageNotFound(String),
}

pub type Result<T> = std::result::Result<T, WikiError>;

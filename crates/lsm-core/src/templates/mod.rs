//! Project templates (specs §Templates).
//!
//! Backed by the embedded `assets/templates/templates.toml` registry.

use serde::{Deserialize, Serialize};

use crate::domain::SiteType;

const TEMPLATES_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/templates/templates.toml"
));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub name: String,
    pub runtime: String,
    #[serde(rename = "site_type")]
    pub site_type_str: String,
    pub install: String,
}

impl ProjectTemplate {
    pub fn site_type(&self) -> SiteType {
        SiteType::parse(&self.site_type_str).unwrap_or(SiteType::Static)
    }
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(rename = "template")]
    templates: Vec<ProjectTemplate>,
}

/// Load all templates from the embedded registry.
pub fn all() -> Vec<ProjectTemplate> {
    let reg: RegistryFile = toml::from_str(TEMPLATES_TOML).expect("templates.toml is valid");
    reg.templates
}

/// Find a template by name.
pub fn find(name: &str) -> Option<ProjectTemplate> {
    all().into_iter().find(|t| t.name == name)
}

/// Names of all available templates.
pub fn names() -> Vec<String> {
    all().into_iter().map(|t| t.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_loads() {
        let t = all();
        assert!(t.len() >= 12, "expected >=12 templates, got {}", t.len());
        let names: Vec<_> = t.iter().map(|x| x.name.as_str()).collect();
        assert!(names.contains(&"laravel"));
        assert!(names.contains(&"go-gin"));
    }

    #[test]
    fn find_maps_site_type() {
        let l = find("laravel").unwrap();
        assert_eq!(l.site_type(), SiteType::Php);
        let n = find("nextjs").unwrap();
        assert_eq!(n.site_type(), SiteType::Proxy);
    }
}

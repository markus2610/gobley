/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/.
 */

use std::{collections::HashMap, fs::File, io::Write, process::Command};

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;
use uniffi_bindgen::{BindingGenerator, Component, ComponentInterface, GenerationSettings};

mod gen_kotlin_multiplatform;
use gen_kotlin_multiplatform::{generate_bindings, Config, ConfigKotlinTarget};

pub struct KotlinBindingGenerator {
    pub force_multiplatform: bool,
}

impl Default for KotlinBindingGenerator {
    fn default() -> Self {
        Self {
            force_multiplatform: false,
        }
    }
}

impl KotlinBindingGenerator {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_multiplatform(mut self, enabled: bool) -> Self {
        self.force_multiplatform = enabled;
        self
    }
}

impl BindingGenerator for KotlinBindingGenerator {
    type Config = Config;

    fn new_config(&self, root_toml: &toml::value::Value) -> Result<Self::Config> {
        // Support both [bindings.kotlin] format and flat format for compatibility
        let kotlin_config = root_toml
            .get("bindings")
            .and_then(|b| b.get("kotlin"))
            .unwrap_or(root_toml);
        
        let mut config: Config = kotlin_config.clone().try_into()?;
        
        // Override with CLI flag if provided
        if self.force_multiplatform {
            config.kotlin_multiplatform = true;
            // Ensure we have default targets if none specified
            if config.kotlin_targets.is_empty() {
                config.kotlin_targets = vec![
                    ConfigKotlinTarget::Jvm,
                    ConfigKotlinTarget::Android,
                    ConfigKotlinTarget::Native,
                ];
            }
        }
        
        Ok(config)
    }

    fn update_component_configs(
        &self,
        settings: &GenerationSettings,
        components: &mut Vec<Component<Self::Config>>,
    ) -> Result<()> {
        for c in &mut *components {
            c.config
                .package_name
                .get_or_insert_with(|| format!("uniffi.{}", c.ci.namespace()));
            c.config.cdylib_name.get_or_insert_with(|| {
                settings
                    .cdylib
                    .clone()
                    .unwrap_or_else(|| format!("uniffi_{}", c.ci.namespace()))
            });
        }
        // We need to update package names
        let packages = HashMap::<String, String>::from_iter(
            components
                .iter()
                .map(|c| (c.ci.crate_name().to_string(), c.config.package_name())),
        );
        for c in components {
            for (ext_crate, ext_package) in &packages {
                if ext_crate != c.ci.crate_name()
                    && !c.config.external_packages.contains_key(ext_crate)
                {
                    c.config
                        .external_packages
                        .insert(ext_crate.to_string(), ext_package.clone());
                }
            }
        }
        Ok(())
    }

    fn write_bindings(
        &self,
        settings: &GenerationSettings,
        components: &[Component<Self::Config>],
    ) -> Result<()> {
        for Component { ci, config, .. } in components {
            let bindings = generate_bindings(config, ci)?;

            write_bindings_target(ci, settings, config, "common", bindings.common);

            if let Some(jvm) = bindings.jvm {
                write_bindings_target(ci, settings, config, "jvm", jvm);
            }
            if let Some(android) = bindings.android {
                write_bindings_target(ci, settings, config, "android", android);
            }
            if let Some(native) = bindings.native {
                write_bindings_target(ci, settings, config, "native", native);
            }
            if let Some(stub) = bindings.stub {
                write_bindings_target(ci, settings, config, "stub", stub);
            }

            if let Some(header) = bindings.header {
                write_cinterop(ci, &settings.out_dir, header);
            }
        }
        Ok(())
    }
}

fn write_bindings_target(
    ci: &ComponentInterface,
    settings: &GenerationSettings,
    config: &Config,
    target: &str,
    content: String,
) {
    let source_set_name = if config.kotlin_multiplatform {
        format!("{}Main", target)
    } else {
        String::from("main")
    };
    let package_path: Utf8PathBuf = config.package_name().split('.').collect();
    let file_name = format!("{}.{}.kt", ci.namespace(), target);

    let dest_dir = Utf8PathBuf::from(&settings.out_dir)
        .join(source_set_name)
        .join("kotlin")
        .join(package_path);
    let file_path = Utf8PathBuf::from(&dest_dir).join(file_name);

    fs::create_dir_all(dest_dir).unwrap();
    fs::write(&file_path, content).unwrap();

    if settings.try_format_code {
        println!("Code generation complete, formatting with ktlint (use --no-format to disable)");
        if let Err(e) = Command::new("ktlint").arg("-F").arg(&file_path).output() {
            println!(
                "Warning: Unable to auto-format {} using ktlint: {e:?}",
                file_path.file_name().unwrap(),
            );
        }
    }
}

fn write_cinterop(ci: &ComponentInterface, out_dir: &Utf8Path, content: String) {
    let dst_dir = Utf8PathBuf::from(out_dir)
        .join("nativeInterop")
        .join("cinterop")
        .join("headers")
        .join(ci.namespace());
    fs::create_dir_all(&dst_dir).unwrap();
    let file_path = dst_dir.join(format!("{}.h", ci.namespace()));
    let mut f = File::create(file_path).unwrap();
    write!(f, "{}", content).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::value::Value;

    #[test]
    fn test_config_parsing_nested_format() {
        let generator = KotlinBindingGenerator::new();
        let toml_str = r#"
        [bindings.kotlin]
        package_name = "com.example.test"
        kotlin_multiplatform = true
        "#;
        let root_toml: Value = toml::from_str(toml_str).unwrap();
        let config = generator.new_config(&root_toml).unwrap();
        
        assert_eq!(config.package_name, Some("com.example.test".to_string()));
        assert!(config.kotlin_multiplatform);
    }

    #[test]
    fn test_config_parsing_flat_format() {
        let generator = KotlinBindingGenerator::new();
        let toml_str = r#"
        package_name = "com.example.test"
        kotlin_multiplatform = true
        "#;
        let root_toml: Value = toml::from_str(toml_str).unwrap();
        let config = generator.new_config(&root_toml).unwrap();
        
        assert_eq!(config.package_name, Some("com.example.test".to_string()));
        assert!(config.kotlin_multiplatform);
    }

    #[test]
    fn test_kmp_flag_overrides_config() {
        let generator = KotlinBindingGenerator::new().with_multiplatform(true);
        let toml_str = r#"
        [bindings.kotlin]
        package_name = "com.example.test"
        kotlin_multiplatform = false
        "#;
        let root_toml: Value = toml::from_str(toml_str).unwrap();
        let config = generator.new_config(&root_toml).unwrap();
        
        // CLI flag should override config file setting
        assert!(config.kotlin_multiplatform);
        // Should have default targets
        assert_eq!(config.kotlin_targets.len(), 3);
    }

    #[test]
    fn test_kotlin_targets_defaults() {
        let generator = KotlinBindingGenerator::new().with_multiplatform(true);
        let toml_str = r#"
        [bindings.kotlin]
        package_name = "com.example.test"
        "#;
        let root_toml: Value = toml::from_str(toml_str).unwrap();
        let config = generator.new_config(&root_toml).unwrap();
        
        // Should have default targets when multiplatform is enabled
        assert!(config.kotlin_multiplatform);
        assert_eq!(config.kotlin_targets.len(), 3);
        assert!(config.kotlin_targets.contains(&ConfigKotlinTarget::Jvm));
        assert!(config.kotlin_targets.contains(&ConfigKotlinTarget::Android));
        assert!(config.kotlin_targets.contains(&ConfigKotlinTarget::Native));
    }

    #[test]
    fn test_explicit_targets_preserved() {
        let generator = KotlinBindingGenerator::new().with_multiplatform(true);
        let toml_str = r#"
        [bindings.kotlin]
        package_name = "com.example.test"
        kotlin_targets = ["jvm"]
        "#;
        let root_toml: Value = toml::from_str(toml_str).unwrap();
        let config = generator.new_config(&root_toml).unwrap();
        
        // Explicit targets should be preserved
        assert_eq!(config.kotlin_targets.len(), 1);
        assert!(config.kotlin_targets.contains(&ConfigKotlinTarget::Jvm));
    }
}

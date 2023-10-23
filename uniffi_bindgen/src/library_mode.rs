/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Alternative implementation for the `generate` command, that we plan to eventually replace the current default with.
///
/// Traditionally, users would invoke `uniffi-bindgen generate` to generate bindings for a single crate, passing it the UDL file, config file, etc.
///
/// library_mode is a new way to generate bindings for multiple crates at once.
/// Users pass the path to the build cdylib file and UniFFI figures everything out, leveraging `cargo_metadata`, the metadata UniFFI stores inside exported symbols in the dylib, etc.
///
/// This brings several advantages.:
///   - No more need to specify the dylib in the `uniffi.toml` file(s)
///   - UniFFI can figure out the dependencies based on the dylib exports and generate the sources for
///     all of them at once.
///   - UniFFI can figure out the package/module names for each crate, eliminating the external
///     package maps.
use crate::{
    bindings::TargetLanguage, load_initial_config, macro_metadata, BindingGenerator,
    BindingGeneratorDefault, BindingsConfig, ComponentInterface, Result,
};
use camino::Utf8Path;
use std::fs;
use uniffi_meta::{create_metadata_groups, group_metadata};

/// Generate foreign bindings
///
/// Returns the list of sources used to generate the bindings, in no particular order.
pub fn generate_bindings(
    library_path: &Utf8Path,
    crate_root: &Utf8Path,
    target_languages: &[TargetLanguage],
    out_dir: &Utf8Path,
    try_format_code: bool,
) -> Result<Vec<Source<crate::Config>>> {
    generate_external_bindings(
        BindingGeneratorDefault {
            target_languages: target_languages.into(),
            try_format_code,
        },
        library_path,
        crate_root,
        out_dir,
    )
}

/// Generate foreign bindings
///
/// Returns the list of sources used to generate the bindings, in no particular order.
pub fn generate_external_bindings<T: BindingGenerator>(
    binding_generator: T,
    library_path: &Utf8Path,
    crate_root: &Utf8Path,
    out_dir: &Utf8Path,
) -> Result<Vec<Source<T::Config>>> {
    let cdylib_name = calc_cdylib_name(library_path);
    binding_generator.check_library_path(library_path, cdylib_name)?;

    let sources = find_sources(crate_root, library_path, cdylib_name)?;
    fs::create_dir_all(out_dir)?;

    for source in sources.iter() {
        binding_generator.write_bindings(&source.ci, &source.config, out_dir, None)?;
    }

    Ok(sources)
}

// A single source that we generate bindings for
#[derive(Debug)]
pub struct Source<Config: BindingsConfig> {
    pub crate_name: String,
    pub ci: ComponentInterface,
    pub config: Config,
}

// If `library_path` is a C dynamic library, return its name
pub fn calc_cdylib_name(library_path: &Utf8Path) -> Option<&str> {
    let cdylib_extentions = [".so", ".dll", ".dylib"];
    let filename = library_path.file_name()?;
    let filename = filename.strip_prefix("lib").unwrap_or(filename);
    for ext in cdylib_extentions {
        if let Some(f) = filename.strip_suffix(ext) {
            return Some(f);
        }
    }
    None
}

fn find_sources<Config: BindingsConfig>(
    // cargo_metadata: &cargo_metadata::Metadata,
    crate_root: &Utf8Path,
    library_path: &Utf8Path,
    cdylib_name: Option<&str>,
) -> Result<Vec<Source<Config>>> {
    let items = macro_metadata::extract_from_library(library_path)?;
    let mut metadata_groups = create_metadata_groups(&items);
    group_metadata(&mut metadata_groups, items)?;

    metadata_groups
        .into_values()
        .map(|group| {
            let crate_name = group.namespace.crate_name.clone();
            let mut ci = ComponentInterface::new(&crate_name);
            ci.add_metadata(group)?;
            let mut config = load_initial_config::<Config>(crate_root, None)?;
            if let Some(cdylib_name) = cdylib_name {
                config.update_from_cdylib_name(cdylib_name);
            }
            config.update_from_ci(&ci);
            Ok(Source {
                config,
                crate_name,
                ci,
            })
        })
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn calc_cdylib_name_is_correct() {
        assert_eq!(
            "uniffi",
            calc_cdylib_name("/path/to/libuniffi.so".into()).unwrap()
        );
        assert_eq!(
            "uniffi",
            calc_cdylib_name("/path/to/libuniffi.dylib".into()).unwrap()
        );
        assert_eq!(
            "uniffi",
            calc_cdylib_name("/path/to/uniffi.dll".into()).unwrap()
        );
    }

    /// Right now we unconditionally strip the `lib` prefix.
    ///
    /// Technically Windows DLLs do not start with a `lib` prefix,
    /// but a library name could start with a `lib` prefix.
    /// On Linux/macOS this would result in a `liblibuniffi.{so,dylib}` file.
    #[test]
    #[ignore] // Currently fails.
    fn calc_cdylib_name_is_correct_on_windows() {
        assert_eq!(
            "libuniffi",
            calc_cdylib_name("/path/to/libuniffi.dll".into()).unwrap()
        );
    }
}

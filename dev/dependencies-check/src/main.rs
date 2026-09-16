
extern crate cargo;

use cargo::CargoResult;
/// Check for circular dependencies between Rustberg crates
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::Path;

use cargo::util::context::GlobalContext;

fn main() -> CargoResult<()> {
    let gctx = GlobalContext::default()?;
    // This is the path for the depcheck binary
    let path = env::var("CARGO_MANIFEST_DIR").unwrap();
    let root_cargo_toml = Path::new(&path)
        // dev directory
        .parent()
        .expect("Can not find dev directory")
        // project root directory
        .parent()
        .expect("Can not find project root directory")
        .join("Cargo.toml");

    println!(
        "Checking for circular dependencies in {}",
        root_cargo_toml.display()
    );
    let workspace = cargo::core::Workspace::new(&root_cargo_toml, &gctx)?;
    let (_, resolve) = cargo::ops::resolve_ws(&workspace, false)?;

    let mut package_deps = HashMap::new();
    for package_id in resolve
        .iter()
        .filter(|id| id.name().starts_with("datafusion"))
    {
        let deps: Vec<String> = resolve
            .deps(package_id)
            .filter(|(package_id, _)| package_id.name().starts_with("datafusion"))
            .map(|(package_id, _)| package_id.name().to_string())
            .collect();
        package_deps.insert(package_id.name().to_string(), deps);
    }

    // check for circular dependencies
    for (root_package, deps) in &package_deps {
        let mut seen = HashSet::new();
        for dep in deps {
            check_circular_deps(root_package, dep, &package_deps, &mut seen);
        }
    }
    println!("No circular dependencies found");
    Ok(())
}

fn check_circular_deps(
    root_package: &str,
    current_dep: &str,
    package_deps: &HashMap<String, Vec<String>>,
    seen: &mut HashSet<String>,
) {
    if root_package == current_dep {
        panic!(
            "circular dependency detected from {root_package} to self via one of {:?}",
            seen
        );
    }
    if seen.contains(current_dep) {
        return;
    }
    seen.insert(current_dep.to_string());
    if let Some(deps) = package_deps.get(current_dep) {
        for dep in deps {
            check_circular_deps(root_package, dep, package_deps, seen);
        }
    }
}

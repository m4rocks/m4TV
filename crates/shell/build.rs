//! Builds the Remote web app (ADR 0004), whose `dist/` the Shell embeds. npm runs only when the
//! sources or the lockfile changed since the last build. A build without Node fails, unless
//! `M4TV_SKIP_WEB=1` embeds a "Remote not built" page instead.

use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    process::Command,
};

const NOT_BUILT: &str = "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>m4TV Remote</title><h1>Remote not built</h1>\
<p>This Shell was built with M4TV_SKIP_WEB=1, so it has no Remote web app.</p></html>";

/// What the web app is built from, besides `src/`.
const INPUTS: [&str; 7] = [
    "index.html",
    "package.json",
    "package-lock.json",
    "vite.config.ts",
    "tsconfig.json",
    "postcss.config.js",
    "tailwind.config.js",
];

fn main() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/remote");
    println!("cargo:rerun-if-env-changed=M4TV_SKIP_WEB");
    println!("cargo:rerun-if-changed={}", web.join("src").display());
    for input in INPUTS {
        println!("cargo:rerun-if-changed={}", web.join(input).display());
    }

    let dist = web.join("dist");
    if std::env::var("M4TV_SKIP_WEB").is_ok_and(|v| v == "1") {
        // Keep a real build that is already there.
        if !dist.join("index.html").exists() {
            fs::create_dir_all(&dist).expect("create packages/remote/dist");
            fs::write(dist.join("index.html"), NOT_BUILT).expect("write the placeholder page");
        }
        return;
    }

    let lock = hash(&[web.join("package-lock.json")]);
    let mut sources = vec![web.join("src")];
    sources.extend(INPUTS.iter().map(|input| web.join(input)));
    let sources = hash(&sources);
    let stamp = dist.join(".built");
    let built = fs::read_to_string(&stamp).unwrap_or_default();
    let (built_lock, built_sources) = built.split_once(' ').unwrap_or_default();
    if built_lock == lock && built_sources == sources && dist.join("index.html").exists() {
        return;
    }

    if built_lock != lock || !web.join("node_modules").exists() {
        npm(&web, &["ci"]);
    }
    npm(&web, &["run", "build"]);
    fs::write(&stamp, format!("{lock} {sources}")).expect("write the build stamp");
}

fn npm(dir: &Path, args: &[&str]) {
    let program = if cfg!(windows) { "npm.cmd" } else { "npm" };
    match Command::new(program).args(args).current_dir(dir).status() {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("`npm {}` failed ({status}) in packages/remote", args.join(" ")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => panic!(
            "Node.js (npm) is needed to build the Remote web app (ADR 0004) and was not found. \
             Install Node, or set M4TV_SKIP_WEB=1 to build a Shell without the Remote."
        ),
        Err(e) => panic!("could not run npm: {e}"),
    }
}

/// A hash of the files under `paths` (directories are walked), in a fixed order.
fn hash(paths: &[PathBuf]) -> String {
    let mut files = Vec::new();
    for path in paths {
        collect(path, &mut files);
    }
    files.sort();
    let mut hasher = DefaultHasher::new();
    for file in files {
        file.hash(&mut hasher);
        fs::read(&file).unwrap_or_default().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            collect(&entry.path(), files);
        }
    } else if path.is_file() {
        files.push(path.to_path_buf());
    }
}

//! Builds the Remote web app (ADR 0004), whose `dist/` the Shell embeds. npm runs only when the
//! sources or the lockfile changed since the last build. A build without Node fails, unless
//! `M4TV_SKIP_WEB=1` embeds a "Remote not built" page instead. It also turns the design tokens
//! (ADR 0005) into the constants the Shell draws with.

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

/// What the web app is built from outside its own folder: the tokens Tailwind reads and the
/// fonts Vite copies in.
const DESIGN_INPUTS: [&str; 2] = ["tokens.json", "public"];

fn main() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/remote");
    let design = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../design");
    write_tokens(&design);
    println!("cargo:rerun-if-env-changed=M4TV_SKIP_WEB");
    println!("cargo:rerun-if-changed={}", web.join("src").display());
    for input in INPUTS {
        println!("cargo:rerun-if-changed={}", web.join(input).display());
    }
    for input in DESIGN_INPUTS {
        println!("cargo:rerun-if-changed={}", design.join(input).display());
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
    sources.extend(DESIGN_INPUTS.iter().map(|input| design.join(input)));
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

/// Writes `$OUT_DIR/tokens.rs`: both palettes, and the radii, spacing, type sizes, motion times and
/// Safe area as constants, all from `design/tokens.json`.
fn write_tokens(design: &Path) {
    let text = fs::read_to_string(design.join("tokens.json")).expect("read design/tokens.json");
    let tokens: serde_json::Value = serde_json::from_str(&text).expect("design/tokens.json is JSON");
    let hex = |value: &serde_json::Value| {
        let value = value.as_str().and_then(|v| v.strip_prefix('#'));
        format!("0x{}", value.expect("a colour is a #rrggbb string"))
    };

    let colors = tokens["color"]["dark"].as_object().expect("design/tokens.json has colours");
    let mut out = String::from("/// One theme's colours, as 0xRRGGBB.\n#[derive(Clone, Copy)]\npub struct Palette {\n");
    for name in colors.keys() {
        out += &format!("    pub {name}: u32,\n");
    }
    out += "}\n\n";
    for theme in ["dark", "light"] {
        out += &format!("pub const {}: Palette = Palette {{\n", theme.to_uppercase());
        for name in colors.keys() {
            out += &format!("    {name}: {},\n", hex(&tokens["color"][theme][name]));
        }
        out += "};\n\n";
    }
    // A QR code is black on white in every theme, so a phone reads it from across the room.
    for (name, color) in tokens["qr"].as_object().expect("design/tokens.json has the QR colours") {
        out += &format!("pub const QR_{}: u32 = {};\n", name.to_uppercase(), hex(color));
    }
    let scale = |group: &str, prefix: &str, out: &mut String, size: &dyn Fn(&serde_json::Value) -> f64| {
        for (name, value) in tokens[group].as_object().expect("design/tokens.json has the scales") {
            out.push_str(&format!(
                "pub const {prefix}_{}: f32 = {:?};\n",
                name.to_uppercase(),
                size(value)
            ));
        }
    };
    let number = |v: &serde_json::Value| v.as_f64().expect("a scale holds numbers");
    scale("radius", "RADIUS", &mut out, &number);
    scale("spacing", "SPACE", &mut out, &number);
    scale("type", "TEXT", &mut out, &|v| number(&v[0]));
    for (name, ms) in tokens["motion"].as_object().expect("design/tokens.json has motion") {
        out += &format!("pub const MOTION_{}_MS: u64 = {};\n", name.to_uppercase(), ms);
    }
    out += &format!("pub const OVERLAY_ALPHA: f32 = {:?};\n", number(&tokens["overlay_alpha"]));
    out += &format!("/// The margin for text and Tiles, as a fraction of the screen.\npub const SAFE_AREA: f32 = {:?};\n", number(&tokens["safe_area"]));

    let path = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("tokens.rs");
    fs::write(path, out).expect("write tokens.rs");
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

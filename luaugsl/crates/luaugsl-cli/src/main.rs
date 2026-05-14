use clap::{Parser, Subcommand};
use luaugsl_hir::lower_module;
use luaugsl_parser::parser::Parser as GslParser;
use luaugsl_spirv::SpirvGenerator;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "luaugsl")]
#[command(about = "Luau GPU Shading Language Compiler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a LuauGSL shader to SPIR-V
    Compile {
        /// Input .gsl file
        input: PathBuf,
        /// Output .spv file (default: input.spv)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Generate debug info
        #[arg(long)]
        debug: bool,
    },
    /// Verify a compiled shader artifact
    Verify {
        /// Input .spv or .lga file
        input: PathBuf,
    },
    /// Print shader reflection info
    Info {
        /// Input .spv file
        input: PathBuf,
    },
    /// Test shader compilation for all examples
    Test,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compile { input, output, debug: _ } => {
            let output = output.unwrap_or_else(|| {
                let mut p = input.clone();
                p.set_extension("spv");
                p
            });

            println!("Compiling {}...", input.display());
            let source = match fs::read_to_string(&input) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading {}: {}", input.display(), e);
                    std::process::exit(1);
                }
            };

            match compile_to_spirv(&source) {
                Ok(spirv) => {
                    let bytes = spirv_to_bytes(&spirv);
                    if let Err(e) = fs::write(&output, &bytes) {
                        eprintln!("Error writing {}: {}", output.display(), e);
                        std::process::exit(1);
                    }
                    println!("  -> {} ({} bytes)", output.display(), bytes.len());
                }
                Err(e) => {
                    eprintln!("Compilation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Verify { input } => {
            println!("Verifying {}...", input.display());
            match fs::read(&input) {
                Ok(data) => {
                    if is_valid_spirv(&data) {
                        println!("Valid SPIR-V ({} words)", data.len() / 4);
                    } else {
                        println!("Not a valid SPIR-V file");
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Info { input } => {
            println!("Shader info for {}...", input.display());
            match fs::read(&input) {
                Ok(data) => {
                    print_spirv_info(&data);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Test => {
            run_tests();
        }
    }
}

fn compile_to_spirv(source: &str) -> Result<Vec<u32>, String> {
    // 1. Parse
    let mut module = GslParser::parse(source).map_err(|e| format!("parse error: {}", e))?;

    // 2. Type check
    let mut type_checker = luaugsl_typeck::TypeChecker::new();
    type_checker
        .check_module(&mut module)
        .map_err(|e| format!("type error: {}", e))?;

    // 3. Lower to HIR
    let mut hir = lower_module(&module);

    // 4. Optimize
    luaugsl_opt::optimize_module(&mut hir);

    // 5. Generate SPIR-V
    let mut gen = SpirvGenerator::new();
    let spirv = gen.generate(&hir).map_err(|e| format!("codegen error: {}", e))?;

    Ok(spirv)
}

fn spirv_to_bytes(spirv: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(spirv.len() * 4);
    for word in spirv {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn is_valid_spirv(data: &[u8]) -> bool {
    if data.len() < 20 {
        return false;
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    magic == 0x07230203 // SPIR-V magic number
}

fn print_spirv_info(data: &[u8]) {
    if data.len() < 20 {
        println!("File too small to be SPIR-V");
        return;
    }

    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    println!("Magic: 0x{:08X} ({})", magic,
        if magic == 0x07230203 { "SPIR-V" } else { "Unknown" });

    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let major = (version >> 16) & 0xFF;
    let minor = (version >> 8) & 0xFF;
    println!("Version: {}.{}", major, minor);

    let generator = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
    println!("Generator: {}", generator);

    let bound = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
    println!("ID Bound: {}", bound);

    println!("Size: {} words ({} bytes)", data.len() / 4, data.len());
}

fn run_tests() {
    let examples = vec![
        ("Basic Fragment", include_str!("../../../examples/basic_fragment.gsl")),
        ("Vertex Shader", include_str!("../../../examples/vertex_shader.gsl")),
        ("Compute Shader", include_str!("../../../examples/compute_shader.gsl")),
        ("Instanced Rendering", include_str!("../../../examples/instanced_rendering.gsl")),
        ("Post-Processing", include_str!("../../../examples/post_process.gsl")),
        ("Raymarch SDF", include_str!("../../../examples/raymarch_sdf.gsl")),
    ];

    let mut passed = 0;
    let mut failed = 0;

    for (name, source) in &examples {
        print!("  {} ... ", name);
        match compile_to_spirv(source) {
            Ok(_) => {
                println!("OK");
                passed += 1;
            }
            Err(e) => {
                println!("FAILED: {}", e);
                failed += 1;
            }
        }
    }

    println!("\n{} passed, {} failed", passed, failed);
}

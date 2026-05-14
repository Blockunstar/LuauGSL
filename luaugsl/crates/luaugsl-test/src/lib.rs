/// Test harness for LuauGSL integration tests.
/// All 6 example shaders from the specification should compile successfully.

#[cfg(test)]
mod tests {
    use luaugsl_hir::lower_module;
    use luaugsl_parser::parser::Parser;
    use luaugsl_spirv::SpirvGenerator;

    fn compile_source(source: &str) -> Result<Vec<u32>, String> {
        let mut module = Parser::parse(source).map_err(|e| format!("parse: {}", e))?;

        let mut typeck = luaugsl_typeck::TypeChecker::new();
        typeck.check_module(&mut module).map_err(|e| format!("typeck: {}", e))?;

        let mut hir = lower_module(&module);
        luaugsl_opt::optimize_module(&mut hir);

        let mut gen = SpirvGenerator::new();
        gen.generate(&hir).map_err(|e| format!("spirv: {}", e))
    }

    #[test]
    fn test_basic_fragment() {
        let src = include_str!("../../../examples/basic_fragment.gsl");
        let result = compile_source(src);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_vertex_shader() {
        let src = include_str!("../../../examples/vertex_shader.gsl");
        let result = compile_source(src);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_compute_shader() {
        let src = include_str!("../../../examples/compute_shader.gsl");
        let result = compile_source(src);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_instanced_rendering() {
        let src = include_str!("../../../examples/instanced_rendering.gsl");
        let result = compile_source(src);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_post_process() {
        let src = include_str!("../../../examples/post_process.gsl");
        let result = compile_source(src);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }

    #[test]
    fn test_raymarch_sdf() {
        let src = include_str!("../../../examples/raymarch_sdf.gsl");
        let result = compile_source(src);
        assert!(result.is_ok(), "Failed: {:?}", result.err());
    }
}

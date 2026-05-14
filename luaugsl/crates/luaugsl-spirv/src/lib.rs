use luaugsl_core::ast::{AssignTarget, BinOp, Literal, UnOp};
use luaugsl_core::error::{CompileError, CompileResult};
use luaugsl_core::types::*;
use luaugsl_hir::*;
use rspirv::binary::Assemble;
use rspirv::dr::{Instruction, Module, Operand};
use rspirv::spirv::{Op, Word};
use std::collections::HashMap;

/// SPIR-V code generator state.
pub struct SpirvGenerator {
    module: Module,
    next_id: Word,
    type_ids: HashMap<TypeKey, Word>,
    constant_u32_ids: HashMap<u32, Word>,
    value_ids: HashMap<String, Word>,
    binding_ids: HashMap<String, Word>,
    binding_types: HashMap<String, Type>,
    symbol_types: HashMap<String, Type>,
    value_types: HashMap<Word, Type>,
    current_function: Option<Word>,
    current_block: Option<Word>,
    decorations: Vec<Instruction>,
    /// Accumulates instructions for the function currently being built.
    /// Flushed into module.functions at the end of generate_function.
    func_instrs: Vec<Instruction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
enum TypeKey {
    Void,
    Bool,
    I32,
    U32,
    F32,
    Vec(u32, Word),
    Mat(u32, Word),
    Pointer(rspirv::spirv::StorageClass, Word),
    Function(Word, Vec<Word>),
    Image(Word),
    SampledImage(Word),
    Sampler,
    Array(Word, u32),
    RuntimeArray(Word),
    Struct(Vec<Word>),
    ConstU32(u32),
}

impl SpirvGenerator {
    pub fn new() -> Self {
        SpirvGenerator {
            module: Module::new(),
            next_id: 1,
            type_ids: HashMap::new(),
            constant_u32_ids: HashMap::new(),
            value_ids: HashMap::new(),
            binding_ids: HashMap::new(),
            binding_types: HashMap::new(),
            symbol_types: HashMap::new(),
            value_types: HashMap::new(),
            current_function: None,
            current_block: None,
            decorations: Vec::new(),
            func_instrs: Vec::new(),
        }
    }

    fn fresh_id(&mut self) -> Word {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn add_instr(&mut self, instr: Instruction) {
        if self.current_function.is_some() {
            self.func_instrs.push(instr);
        } else {
            self.module.types_global_values.push(instr);
        }
    }

    fn add_global_instr(&mut self, instr: Instruction) {
        self.module.types_global_values.push(instr);
    }

    pub fn generate(&mut self, hir: &HIRModule) -> CompileResult<Vec<u32>> {
        // 1. Header
        self.emit_header();

        // 2. Capabilities
        self.emit_capability(rspirv::spirv::Capability::Shader);
        if hir.stage == Some(ShaderStage::Compute) {
            self.emit_capability(rspirv::spirv::Capability::Float64);
        }

        // 3. Memory model
        self.emit_memory_model();

        // 4. Pre-declare entry point function IDs
        if let Some(ref entry_name) = hir.entry_point {
            if let Some(func) = hir.functions.iter().find(|f| &f.name == entry_name) {
                let func_id = self.fresh_id();
                self.value_ids.insert(entry_name.clone(), func_id);

                let exec_model = stage_to_execution_model(func.stage.unwrap_or(ShaderStage::Fragment));
                self.module.entry_points.push(Instruction::new(
                    Op::EntryPoint, None, None,
                    vec![
                        Operand::ExecutionModel(exec_model),
                        Operand::IdRef(func_id),
                        Operand::LiteralString(func.name.clone()),
                    ],
                ));

                if func.stage == Some(ShaderStage::Compute) {
                    let wg_size = func.workgroup_size.unwrap_or([1, 1, 1]);
                    self.module.execution_modes.push(Instruction::new(
                        Op::ExecutionMode, None, None,
                        vec![
                            Operand::IdRef(func_id),
                            Operand::ExecutionMode(rspirv::spirv::ExecutionMode::LocalSize),
                            Operand::LiteralInt32(wg_size[0]),
                            Operand::LiteralInt32(wg_size[1]),
                            Operand::LiteralInt32(wg_size[2]),
                        ],
                    ));
                }
            }
        }

        // 5. Declare common types
        let void_id = self.get_type_id(TypeKey::Void);
        let bool_id = self.get_type_id(TypeKey::Bool);
        let i32_id = self.get_type_id(TypeKey::I32);
        let f32_id = self.get_type_id(TypeKey::F32);
        let vec2_id = self.get_type_id(TypeKey::Vec(2, f32_id));
        let vec3_id = self.get_type_id(TypeKey::Vec(3, f32_id));
        let vec4_id = self.get_type_id(TypeKey::Vec(4, f32_id));
        let _ = (void_id, bool_id, i32_id, vec2_id, vec3_id, vec4_id);

        // 6. Record and declare bindings
        for binding in &hir.bindings {
            self.binding_types.insert(binding.name.clone(), binding.ty.clone());
            self.declare_binding(binding);
        }

        // 7. Declare and generate functions
        for func in &hir.functions {
            self.generate_function(func)?;
        }

        // 8. Finalize
        let module = self.build_module();
        Ok(module.assemble())
    }

    fn emit_header(&mut self) {
        self.module.header = Some(rspirv::dr::ModuleHeader::new(0));
        // Import GLSL.std.450 for extended instructions
        let ext_id = self.fresh_id();
        self.module.ext_inst_imports.push(Instruction::new(
            Op::ExtInstImport, Some(ext_id), None,
            vec![Operand::LiteralString("GLSL.std.450".into())],
        ));
        self.value_ids.insert("GLSL.std.450".into(), ext_id);
    }

    fn emit_capability(&mut self, cap: rspirv::spirv::Capability) {
        self.module.capabilities.push(Instruction::new(
            Op::Capability, None, None,
            vec![Operand::Capability(cap)],
        ));
    }

    fn emit_memory_model(&mut self) {
        self.module.memory_model = Some(Instruction::new(
            Op::MemoryModel, None, None,
            vec![
                Operand::AddressingModel(rspirv::spirv::AddressingModel::Logical),
                Operand::MemoryModel(rspirv::spirv::MemoryModel::GLSL450),
            ],
        ));
    }

    fn declare_binding(&mut self, binding: &luaugsl_core::types::Binding) {
        let storage_class = match &binding.ty {
            Type::Texture2D | Type::TextureCube | Type::Texture3D
            | Type::Texture2DArray | Type::TextureCubeArray
            | Type::StorageImage { .. } | Type::Sampler => rspirv::spirv::StorageClass::UniformConstant,
            Type::UniformBuffer(_) => rspirv::spirv::StorageClass::Uniform,
            Type::StorageBuffer { .. } => rspirv::spirv::StorageClass::StorageBuffer,
            _ => rspirv::spirv::StorageClass::UniformConstant,
        };

        // For UniformBuffer, add Block + member offsets so the block is layout-valid.
        if let Type::UniformBuffer(fields) = &binding.ty {
            let struct_ty = self.type_to_id(&binding.ty);
            self.decorations.push(Instruction::new(
                Op::Decorate, None, None,
                vec![
                    Operand::IdRef(struct_ty),
                    Operand::Decoration(rspirv::spirv::Decoration::Block),
                ],
            ));

            let mut offset = 0u32;
            for (i, field) in fields.iter().enumerate() {
                let field_offset = field.offset.unwrap_or_else(|| {
                    let align = field.align.unwrap_or_else(|| self.std140_align(&field.ty));
                    offset = align_to(offset, align.max(4));
                    let current = offset;
                    offset += self.std140_size(&field.ty).max(align);
                    current
                });
                self.decorations.push(Instruction::new(
                    Op::MemberDecorate, None, None,
                    vec![
                        Operand::IdRef(struct_ty),
                        Operand::LiteralInt32(i as u32),
                        Operand::Decoration(rspirv::spirv::Decoration::Offset),
                        Operand::LiteralInt32(field_offset),
                    ],
                ));
                if field.offset.is_some() {
                    offset = field_offset + self.std140_size(&field.ty);
                }
            }
        }

        let ptr_ty = self.binding_ptr_type(&binding.ty);
        let var_id = self.fresh_id();
        self.module.types_global_values.push(Instruction::new(
            Op::Variable,
            Some(ptr_ty), Some(var_id),
            vec![Operand::StorageClass(storage_class)],
        ));
        self.binding_ids.insert(binding.name.clone(), var_id);

        self.decorations.push(Instruction::new(
            Op::Decorate, None, None,
            vec![
                Operand::IdRef(var_id),
                Operand::Decoration(rspirv::spirv::Decoration::Binding),
                Operand::LiteralInt32(binding.binding),
            ],
        ));
        self.decorations.push(Instruction::new(
            Op::Decorate, None, None,
            vec![
                Operand::IdRef(var_id),
                Operand::Decoration(rspirv::spirv::Decoration::DescriptorSet),
                Operand::LiteralInt32(binding.set),
            ],
        ));
    }

    fn binding_ptr_type(&mut self, ty: &Type) -> Word {
        let storage_class = match ty {
            // Textures, samplers, and combined image samplers use UniformConstant
            Type::Texture2D | Type::TextureCube | Type::Texture3D
            | Type::Texture2DArray | Type::TextureCubeArray
            | Type::StorageImage { .. } | Type::Sampler => rspirv::spirv::StorageClass::UniformConstant,
            // Uniform buffers use the Uniform storage class
            Type::UniformBuffer(_) => rspirv::spirv::StorageClass::Uniform,
            // Storage buffers use StorageBuffer storage class
            Type::StorageBuffer { .. } => rspirv::spirv::StorageClass::StorageBuffer,
            // Default to UniformConstant for anything else
            _ => rspirv::spirv::StorageClass::UniformConstant,
        };
        let inner = self.type_to_id(ty);
        self.get_type_id(TypeKey::Pointer(storage_class, inner))
    }

    fn type_to_id(&mut self, ty: &Type) -> Word {
        let key = match ty {
            Type::Nil => TypeKey::Void,
            Type::Bool => TypeKey::Bool,
            Type::I32 => TypeKey::I32,
            Type::U32 => TypeKey::U32,
            Type::F32 | Type::Number => TypeKey::F32,
            Type::Vec2 => TypeKey::Vec(2, self.get_type_id(TypeKey::F32)),
            Type::Vec3 => TypeKey::Vec(3, self.get_type_id(TypeKey::F32)),
            Type::Vec4 => TypeKey::Vec(4, self.get_type_id(TypeKey::F32)),
            Type::Vec2i => TypeKey::Vec(2, self.get_type_id(TypeKey::I32)),
            Type::Vec3i => TypeKey::Vec(3, self.get_type_id(TypeKey::I32)),
            Type::Vec4i => TypeKey::Vec(4, self.get_type_id(TypeKey::I32)),
            Type::Vec2u => TypeKey::Vec(2, self.get_type_id(TypeKey::U32)),
            Type::Vec3u => TypeKey::Vec(3, self.get_type_id(TypeKey::U32)),
            Type::Vec4u => TypeKey::Vec(4, self.get_type_id(TypeKey::U32)),
            Type::BVec2 => TypeKey::Vec(2, self.get_type_id(TypeKey::Bool)),
            Type::BVec3 => TypeKey::Vec(3, self.get_type_id(TypeKey::Bool)),
            Type::BVec4 => TypeKey::Vec(4, self.get_type_id(TypeKey::Bool)),
            Type::Mat2x2 => {
                let f32_id = self.get_type_id(TypeKey::F32);
                let vec2_id = self.get_type_id(TypeKey::Vec(2, f32_id));
                TypeKey::Mat(2, vec2_id)
            }
            Type::Mat3x3 => {
                let f32_id = self.get_type_id(TypeKey::F32);
                let vec3_id = self.get_type_id(TypeKey::Vec(3, f32_id));
                TypeKey::Mat(3, vec3_id)
            }
            Type::Mat4x4 => {
                let f32_id = self.get_type_id(TypeKey::F32);
                let vec4_id = self.get_type_id(TypeKey::Vec(4, f32_id));
                TypeKey::Mat(4, vec4_id)
            }
            Type::Texture2D | Type::TextureCube | Type::Texture3D
            | Type::Texture2DArray | Type::TextureCubeArray | Type::StorageImage { .. } => {
                let f32_id = self.get_type_id(TypeKey::F32);
                TypeKey::Image(f32_id)
            }
            Type::Sampler => TypeKey::Sampler,
            Type::UniformBuffer(fields) => {
                // Build a SPIR-V struct type for the uniform buffer fields
                let member_ids: Vec<Word> = fields.iter()
                    .map(|f| self.type_to_id(&f.ty))
                    .collect();
                TypeKey::Struct(member_ids)
            }
            Type::StorageBuffer { elem_ty, .. } => {
                // Storage buffers use a runtime array wrapped in a struct.
                let elem_id = self.type_to_id(elem_ty);
                let runtime_array = self.get_type_id(TypeKey::RuntimeArray(elem_id));
                TypeKey::Struct(vec![runtime_array])
            }
            Type::Array(elem_ty, Some(count)) => {
                let elem_id = self.type_to_id(elem_ty);
                TypeKey::Array(elem_id, *count)
            }
            Type::Array(elem_ty, None) => {
                let elem_id = self.type_to_id(elem_ty);
                TypeKey::RuntimeArray(elem_id)
            }
            _ => TypeKey::Void,
        };
        self.get_type_id(key)
    }

    fn get_type_id(&mut self, key: TypeKey) -> Word {
        if let Some(&id) = self.type_ids.get(&key) {
            return id;
        }
        let id = self.fresh_id();
        self.type_ids.insert(key.clone(), id);

        match key {
            TypeKey::Void => {
                self.add_global_instr(Instruction::new(Op::TypeVoid, Some(id), None, vec![]));
            }
            TypeKey::Bool => {
                self.add_global_instr(Instruction::new(Op::TypeBool, Some(id), None, vec![]));
            }
            TypeKey::I32 => {
                self.add_global_instr(Instruction::new(Op::TypeInt, Some(id), None, vec![
                    Operand::LiteralInt32(32),
                    Operand::LiteralInt32(1),
                ]));
            }
            TypeKey::U32 => {
                self.add_global_instr(Instruction::new(Op::TypeInt, Some(id), None, vec![
                    Operand::LiteralInt32(32),
                    Operand::LiteralInt32(0),
                ]));
            }
            TypeKey::F32 => {
                self.add_global_instr(Instruction::new(Op::TypeFloat, Some(id), None, vec![
                    Operand::LiteralInt32(32),
                ]));
            }
            TypeKey::Vec(n, elem) => {
                self.add_global_instr(Instruction::new(Op::TypeVector, Some(id), None, vec![
                    Operand::IdRef(elem),
                    Operand::LiteralInt32(n),
                ]));
            }
            TypeKey::Mat(n, col_ty) => {
                self.add_global_instr(Instruction::new(Op::TypeMatrix, Some(id), None, vec![
                    Operand::IdRef(col_ty),
                    Operand::LiteralInt32(n),
                ]));
            }
            TypeKey::Pointer(storage, inner) => {
                self.add_global_instr(Instruction::new(Op::TypePointer, Some(id), None, vec![
                    Operand::StorageClass(storage),
                    Operand::IdRef(inner),
                ]));
            }
            TypeKey::Function(ret, ref params) => {
                let mut operands = vec![Operand::IdRef(ret)];
                for p in params {
                    operands.push(Operand::IdRef(*p));
                }
                self.add_global_instr(Instruction::new(Op::TypeFunction, Some(id), None, operands));
            }
            TypeKey::Image(sampled_type) => {
                self.add_global_instr(Instruction::new(Op::TypeImage, Some(id), None, vec![
                    Operand::IdRef(sampled_type),
                    Operand::Dim(rspirv::spirv::Dim::Dim2D),
                    Operand::LiteralInt32(0),
                    Operand::LiteralInt32(0),
                    Operand::LiteralInt32(0),
                    Operand::LiteralInt32(1),
                    Operand::ImageFormat(rspirv::spirv::ImageFormat::Rgba8),
                ]));
            }
            TypeKey::Sampler => {
                self.add_global_instr(Instruction::new(Op::TypeSampler, Some(id), None, vec![]));
            }
            TypeKey::SampledImage(image_ty) => {
                self.add_global_instr(Instruction::new(Op::TypeSampledImage, Some(id), None, vec![
                    Operand::IdRef(image_ty),
                ]));
            }
            TypeKey::Array(elem, count) => {
                let const_id = self.get_u32_constant_id(count);
                self.add_global_instr(Instruction::new(Op::TypeArray, Some(id), None, vec![
                    Operand::IdRef(elem),
                    Operand::IdRef(const_id),
                ]));
            }
            TypeKey::RuntimeArray(elem) => {
                self.add_global_instr(Instruction::new(Op::TypeRuntimeArray, Some(id), None, vec![
                    Operand::IdRef(elem),
                ]));
            }
            TypeKey::ConstU32(value) => {
                let u32_id = self.get_type_id(TypeKey::U32);
                self.add_global_instr(Instruction::new(Op::Constant, Some(u32_id), Some(id), vec![
                    Operand::LiteralInt32(value),
                ]));
            }
            TypeKey::Struct(ref members) => {
                let operands: Vec<_> = members.iter().map(|&m| Operand::IdRef(m)).collect();
                self.add_global_instr(Instruction::new(Op::TypeStruct, Some(id), None, operands));
            }
        }
        id
    }

    fn generate_function(&mut self, func: &HIRFunction) -> CompileResult<()> {
        let ret_id = self.type_to_id(&func.return_type);
        let func_id = if let Some(&id) = self.value_ids.get(&func.name) { id } else { self.fresh_id() };

        let param_types: Vec<_> = func.params.iter().map(|p| self.type_to_id(&p.ty)).collect();
        let func_type_id = self.get_type_id(TypeKey::Function(ret_id, param_types.clone()));

        // Mark that we're inside a function so add_instr routes to func_instrs
        let prev_func = self.current_function;
        let prev_symbols = self.symbol_types.clone();
        self.current_function = Some(func_id);
        self.func_instrs.clear();

        // OpFunction
        self.func_instrs.push(Instruction::new(
            Op::Function, Some(ret_id), Some(func_id),
            vec![Operand::FunctionControl(rspirv::spirv::FunctionControl::NONE),
                 Operand::IdRef(func_type_id)],
        ));

        // OpFunctionParameter instructions
        let mut prev_values = Vec::new();
        for (i, param) in func.params.iter().enumerate() {
            let pid = self.fresh_id();
            if let Some(old) = self.value_ids.insert(param.name.clone(), pid) {
                prev_values.push((param.name.clone(), Some(old)));
            } else {
                prev_values.push((param.name.clone(), None));
            }
            self.func_instrs.push(Instruction::new(
                Op::FunctionParameter, Some(param_types[i]), Some(pid), vec![],
            ));
            self.symbol_types.insert(param.name.clone(), param.ty.clone());
            self.value_types.insert(pid, param.ty.clone());
        }

        // Body: entry label + statements
        let entry_block = self.fresh_id();
        self.current_block = Some(entry_block);
        self.func_instrs.push(Instruction::new(
            Op::Label, None, Some(entry_block), vec![],
        ));

        for stmt in &func.body {
            self.generate_statement(stmt)?;
        }

        // Fallback terminator
        if func.return_type == Type::Nil {
            self.func_instrs.push(Instruction::new(Op::Return, None, None, vec![]));
        } else {
            self.func_instrs.push(Instruction::new(Op::Unreachable, None, None, vec![]));
        }

        // OpFunctionEnd
        self.func_instrs.push(Instruction::new(
            Op::FunctionEnd, None, None, vec![],
        ));

        // Flush func_instrs into module.types_global_values after all type/var globals.
        // rspirv assembles module.functions separately, but the flat DR builder puts
        // everything in types_global_values. We do the same — just append after globals.
        let instrs = std::mem::take(&mut self.func_instrs);
        for instr in instrs {
            self.module.types_global_values.push(instr);
        }

        // Restore state
        for (name, old) in prev_values {
            match old {
                Some(v) => { self.value_ids.insert(name, v); }
                None => { self.value_ids.remove(&name); }
            }
        }
        self.current_function = prev_func;
        self.current_block = None;
        self.symbol_types = prev_symbols;

        Ok(())
    }

    fn generate_statement(&mut self, stmt: &HIRStmt) -> CompileResult<()> {
        match stmt {
            HIRStmt::Declare { name, ty, value, .. } => {
                let val_id = self.generate_expr(value)?;
                self.value_ids.insert(name.clone(), val_id);
                self.symbol_types.insert(name.clone(), ty.clone());
                let record_ty = self.value_types.get(&val_id).cloned().unwrap_or(ty.clone());
                self.value_types.insert(val_id, record_ty);
                Ok(())
            }
            HIRStmt::Assign { target, value } => {
                let val_id = self.generate_expr(value)?;
                match target {
                    AssignTarget::Var(name) => {
                        self.value_ids.insert(name.clone(), val_id);
                    }
                    _ => {}
                }
                Ok(())
            }
            HIRStmt::CompoundAssign { target, op, value } => {
                let val_id = self.generate_expr(value)?;
                match target {
                    AssignTarget::Var(name) => {
                        if let Some(&existing) = self.value_ids.get(name) {
                            let result = self.emit_binary(*op, existing, val_id)?;
                            self.value_ids.insert(name.clone(), result);
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
            HIRStmt::Expr(expr) => {
                self.generate_expr(expr)?;
                Ok(())
            }
            HIRStmt::If { cond, then_branch, else_branch } => {
                let cond_id = self.generate_expr(cond)?;
                let then_block = self.fresh_id();
                let else_block = self.fresh_id();
                let merge_block = self.fresh_id();

                self.add_instr(Instruction::new(
                    Op::SelectionMerge, None, None,
                    vec![Operand::IdRef(merge_block), Operand::SelectionControl(rspirv::spirv::SelectionControl::NONE)],
                ));
                self.add_instr(Instruction::new(
                    Op::BranchConditional, None, None,
                    vec![Operand::IdRef(cond_id), Operand::IdRef(then_block), Operand::IdRef(else_block)],
                ));

                self.current_block = Some(then_block);
                self.add_instr(Instruction::new(Op::Label, None, Some(then_block), vec![]));
                for s in then_branch { self.generate_statement(s)?; }
                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(merge_block)]));

                self.current_block = Some(else_block);
                self.add_instr(Instruction::new(Op::Label, None, Some(else_block), vec![]));
                for s in else_branch { self.generate_statement(s)?; }
                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(merge_block)]));

                self.current_block = Some(merge_block);
                self.add_instr(Instruction::new(Op::Label, None, Some(merge_block), vec![]));

                Ok(())
            }
            HIRStmt::Loop { cond, body } => {
                let header = self.fresh_id();
                let body_block = self.fresh_id();
                let merge = self.fresh_id();
                let continue_block = self.fresh_id();

                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(header)]));

                self.current_block = Some(header);
                self.add_instr(Instruction::new(Op::Label, None, Some(header), vec![]));
                self.add_instr(Instruction::new(
                    Op::LoopMerge, None, None,
                    vec![Operand::IdRef(merge), Operand::IdRef(continue_block), Operand::LoopControl(rspirv::spirv::LoopControl::NONE)],
                ));

                if let Some(c) = cond {
                    let c_id = self.generate_expr(c)?;
                    self.add_instr(Instruction::new(
                        Op::BranchConditional, None, None,
                        vec![Operand::IdRef(c_id), Operand::IdRef(body_block), Operand::IdRef(merge)],
                    ));
                } else {
                    self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(body_block)]));
                }

                self.current_block = Some(body_block);
                self.add_instr(Instruction::new(Op::Label, None, Some(body_block), vec![]));
                for s in body { self.generate_statement(s)?; }
                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(continue_block)]));

                self.current_block = Some(continue_block);
                self.add_instr(Instruction::new(Op::Label, None, Some(continue_block), vec![]));
                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(header)]));

                self.current_block = Some(merge);
                self.add_instr(Instruction::new(Op::Label, None, Some(merge), vec![]));

                Ok(())
            }
            HIRStmt::For { var, start, end, step, body } => {
                let start_id = self.generate_expr(start)?;
                let end_id = self.generate_expr(end)?;
                let step_id = self.generate_expr(step)?;
                let i32_id = self.get_type_id(TypeKey::I32);

                self.value_ids.insert(var.clone(), start_id);

                let header = self.fresh_id();
                let body_block = self.fresh_id();
                let merge = self.fresh_id();
                let continue_block = self.fresh_id();

                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(header)]));

                self.current_block = Some(header);
                self.add_instr(Instruction::new(Op::Label, None, Some(header), vec![]));
                self.add_instr(Instruction::new(
                    Op::LoopMerge, None, None,
                    vec![Operand::IdRef(merge), Operand::IdRef(continue_block), Operand::LoopControl(rspirv::spirv::LoopControl::NONE)],
                ));

                let ivar = *self.value_ids.get(var).unwrap_or(&start_id);
                let cmp_id = self.fresh_id();
                self.add_instr(Instruction::new(
                    Op::SLessThan, Some(i32_id), Some(cmp_id),
                    vec![Operand::IdRef(ivar), Operand::IdRef(end_id)],
                ));
                self.add_instr(Instruction::new(
                    Op::BranchConditional, None, None,
                    vec![Operand::IdRef(cmp_id), Operand::IdRef(body_block), Operand::IdRef(merge)],
                ));

                self.current_block = Some(body_block);
                self.add_instr(Instruction::new(Op::Label, None, Some(body_block), vec![]));
                for s in body { self.generate_statement(s)?; }
                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(continue_block)]));

                self.current_block = Some(continue_block);
                self.add_instr(Instruction::new(Op::Label, None, Some(continue_block), vec![]));
                let next_id = self.fresh_id();
                let ivar2 = *self.value_ids.get(var).unwrap_or(&start_id);
                self.add_instr(Instruction::new(
                    Op::IAdd, Some(i32_id), Some(next_id),
                    vec![Operand::IdRef(ivar2), Operand::IdRef(step_id)],
                ));
                self.value_ids.insert(var.clone(), next_id);
                self.add_instr(Instruction::new(Op::Branch, None, None, vec![Operand::IdRef(header)]));

                self.current_block = Some(merge);
                self.add_instr(Instruction::new(Op::Label, None, Some(merge), vec![]));

                Ok(())
            }
            HIRStmt::Return(exprs) => {
                if exprs.is_empty() {
                    self.add_instr(Instruction::new(Op::Return, None, None, vec![]));
                } else {
                    let val_id = self.generate_expr(&exprs[0])?;
                    self.add_instr(Instruction::new(
                        Op::ReturnValue, None, None,
                        vec![Operand::IdRef(val_id)],
                    ));
                }
                Ok(())
            }
            HIRStmt::Break => {
                // Break target is handled by the loop construct; emit OpUnreachable as placeholder
                self.add_instr(Instruction::new(Op::Unreachable, None, None, vec![]));
                Ok(())
            }
            HIRStmt::Continue => {
                // Continue target is handled by the loop construct; emit OpUnreachable as placeholder
                self.add_instr(Instruction::new(Op::Unreachable, None, None, vec![]));
                Ok(())
            }
            HIRStmt::WorkgroupBarrier => {
                self.add_instr(Instruction::new(
                    Op::ControlBarrier, None, None,
                    vec![
                        Operand::Scope(rspirv::spirv::Scope::Workgroup),
                        Operand::Scope(rspirv::spirv::Scope::Workgroup),
                        Operand::MemorySemantics(rspirv::spirv::MemorySemantics::ACQUIRE_RELEASE),
                    ],
                ));
                Ok(())
            }
            HIRStmt::StorageBarrier => {
                self.add_instr(Instruction::new(
                    Op::MemoryBarrier, None, None,
                    vec![
                        Operand::Scope(rspirv::spirv::Scope::Workgroup),
                        Operand::MemorySemantics(rspirv::spirv::MemorySemantics::ACQUIRE_RELEASE | rspirv::spirv::MemorySemantics::UNIFORM_MEMORY),
                    ],
                ));
                Ok(())
            }
            HIRStmt::TextureBarrier => {
                self.add_instr(Instruction::new(
                    Op::MemoryBarrier, None, None,
                    vec![
                        Operand::Scope(rspirv::spirv::Scope::Workgroup),
                        Operand::MemorySemantics(rspirv::spirv::MemorySemantics::ACQUIRE_RELEASE | rspirv::spirv::MemorySemantics::IMAGE_MEMORY),
                    ],
                ));
                Ok(())
            }
            HIRStmt::MemoryBarrier => {
                self.add_instr(Instruction::new(
                    Op::MemoryBarrier, None, None,
                    vec![
                        Operand::Scope(rspirv::spirv::Scope::Device),
                        Operand::MemorySemantics(rspirv::spirv::MemorySemantics::ACQUIRE_RELEASE),
                    ],
                ));
                Ok(())
            }
            HIRStmt::Discard => {
                self.add_instr(Instruction::new(Op::Kill, None, None, vec![]));
                Ok(())
            }
            HIRStmt::Block(stmts) => {
                for s in stmts { self.generate_statement(s)?; }
                Ok(())
            }
        }
    }

    fn generate_expr(&mut self, expr: &HIRExpr) -> CompileResult<Word> {
        match expr {
            HIRExpr::Literal(Literal::Nil) => {
                let id = self.fresh_id();
                let f32_id = self.get_type_id(TypeKey::F32);
                self.add_global_instr(Instruction::new(Op::Undef, Some(f32_id), Some(id), vec![]));
                self.value_types.insert(id, Type::F32);
                Ok(id)
            },
            HIRExpr::Literal(Literal::Bool(false)) => {
                let id = self.fresh_id();
                let bool_id = self.get_type_id(TypeKey::Bool);
                self.add_global_instr(Instruction::new(Op::ConstantFalse, Some(bool_id), Some(id), vec![]));
                self.value_types.insert(id, Type::Bool);
                Ok(id)
            }
            HIRExpr::Literal(Literal::Bool(true)) => {
                let id = self.fresh_id();
                let bool_id = self.get_type_id(TypeKey::Bool);
                self.add_global_instr(Instruction::new(Op::ConstantTrue, Some(bool_id), Some(id), vec![]));
                self.value_types.insert(id, Type::Bool);
                Ok(id)
            }
            HIRExpr::Literal(Literal::Number(n)) => {
                let id = self.fresh_id();
                let f32_id = self.get_type_id(TypeKey::F32);
                self.add_global_instr(Instruction::new(Op::Constant, Some(f32_id), Some(id), vec![
                    Operand::LiteralInt32((*n as f32).to_bits()),
                ]));
                self.value_types.insert(id, Type::F32);
                Ok(id)
            }
            HIRExpr::Literal(Literal::String(_)) => {
                let id = self.fresh_id();
                let f32_id = self.get_type_id(TypeKey::F32);
                self.add_global_instr(Instruction::new(Op::Undef, Some(f32_id), Some(id), vec![]));
                self.value_types.insert(id, Type::F32);
                Ok(id)
            }
            HIRExpr::Local(name) => {
                // Fast path: locally-scoped variable.
                if let Some(&id) = self.value_ids.get(name) {
                    return Ok(id);
                }
                // Fallback: the HIR lowerer emits every identifier as Local, so
                // binding/global names end up here too. Handle them identically to
                // HIRExpr::Global so textures, samplers, and uniforms are resolved.
                if let Some(&binding_id) = self.binding_ids.get(name) {
                    let ty = self.binding_types.get(name)
                        .cloned()
                        .ok_or_else(|| CompileError::UnknownIdentifier(name.clone()))?;
                    let ty_id = self.type_to_id(&ty);
                    let result_id = self.fresh_id();
                    self.add_instr(Instruction::new(
                        Op::Load, Some(ty_id), Some(result_id),
                        vec![Operand::IdRef(binding_id)],
                    ));
                    self.value_types.insert(result_id, ty);
                    return Ok(result_id);
                }
                Err(CompileError::UnknownIdentifier(name.clone()))
            }
            HIRExpr::Global(name) => {
                if let Some(&binding_id) = self.binding_ids.get(name) {
                    let ty = self.binding_types.get(name)
                        .cloned()
                        .ok_or_else(|| CompileError::UnknownIdentifier(name.clone()))?;
                    let ty_id = self.type_to_id(&ty);
                    let result_id = self.fresh_id();
                    self.add_instr(Instruction::new(
                        Op::Load, Some(ty_id), Some(result_id),
                        vec![Operand::IdRef(binding_id)],
                    ));
                    self.value_types.insert(result_id, ty);
                    Ok(result_id)
                } else {
                    self.value_ids.get(name).copied()
                        .ok_or_else(|| CompileError::UnknownIdentifier(name.clone()))
                }
            }
            HIRExpr::Binary { op, left, right } => {
                let l_id = self.generate_expr(left)?;
                let r_id = self.generate_expr(right)?;
                self.emit_binary(*op, l_id, r_id)
            }
            HIRExpr::Unary { op, operand } => {
                let val = self.generate_expr(operand)?;
                self.emit_unary(*op, val)
            }
            HIRExpr::Call { func, args } => {
                let arg_ids: Result<Vec<_>, _> = args.iter().map(|a| self.generate_expr(a)).collect();
                let arg_ids = arg_ids?;
                self.emit_intrinsic_call(func, &arg_ids)
            }
            HIRExpr::NamespacedCall { namespace, func, args } => {
                let arg_ids: Result<Vec<_>, _> = args.iter().map(|a| self.generate_expr(a)).collect();
                let arg_ids = arg_ids?;
                self.emit_intrinsic_call(&format!("{}.{}", namespace, func), &arg_ids)
            }
            HIRExpr::FieldAccess { base, field } => {
                let base_id = self.generate_expr(base)?;
                if is_swizzle(field) {
                    let f32_id = self.get_type_id(TypeKey::F32);
                    let indices: Vec<_> = field.chars().map(|c| match c {
                        'x' | 'r' | 's' => 0,
                        'y' | 'g' | 't' => 1,
                        'z' | 'b' | 'p' => 2,
                        'w' | 'a' | 'q' => 3,
                        _ => 0,
                    }).collect();
                    if indices.len() == 1 {
                        let result_id = self.fresh_id();
                        self.add_instr(Instruction::new(
                            Op::CompositeExtract, Some(f32_id), Some(result_id),
                            vec![Operand::IdRef(base_id), Operand::LiteralInt32(indices[0])],
                        ));
                        self.value_types.insert(result_id, Type::F32);
                        Ok(result_id)
                    } else {
                        let vec_ty = match field.len() {
                            2 => self.get_type_id(TypeKey::Vec(2, f32_id)),
                            3 => self.get_type_id(TypeKey::Vec(3, f32_id)),
                            4 => self.get_type_id(TypeKey::Vec(4, f32_id)),
                            _ => return Ok(base_id),
                        };
                        let result_id = self.fresh_id();
                        let mut operands = vec![Operand::IdRef(base_id), Operand::IdRef(base_id)];
                        for idx in &indices {
                            operands.push(Operand::LiteralInt32(*idx));
                        }
                        self.add_instr(Instruction::new(Op::VectorShuffle, Some(vec_ty), Some(result_id), operands));
                        self.value_types.insert(result_id, match field.len() {
                            2 => Type::Vec2,
                            3 => Type::Vec3,
                            4 => Type::Vec4,
                            _ => Type::Vec2,
                        });
                        Ok(result_id)
                    }
                } else if let Some((member_index, member_ty)) = self.resolve_struct_member(base, field) {
                    let result_ty = self.type_to_id(&member_ty);
                    let result_id = self.fresh_id();
                    self.add_instr(Instruction::new(
                        Op::CompositeExtract, Some(result_ty), Some(result_id),
                        vec![Operand::IdRef(base_id), Operand::LiteralInt32(member_index)],
                    ));
                    self.value_types.insert(result_id, member_ty.clone());
                    Ok(result_id)
                } else {
                    Ok(base_id)
                }
            }
            HIRExpr::Index { base, index } => {
                let base_id = self.generate_expr(base)?;
                let index_id = self.generate_expr(index)?;
                let result_id = self.fresh_id();
                let f32_id = self.get_type_id(TypeKey::F32);
                self.add_instr(Instruction::new(
                    Op::VectorExtractDynamic, Some(f32_id), Some(result_id),
                    vec![Operand::IdRef(base_id), Operand::IdRef(index_id)],
                ));
                Ok(result_id)
            }
            HIRExpr::Cast { expr: inner, ty } => {
                let val_id = self.generate_expr(inner)?;
                let result_id = self.fresh_id();
                let target_ty = self.type_to_id(ty);
                self.add_instr(Instruction::new(
                    Op::Bitcast, Some(target_ty), Some(result_id),
                    vec![Operand::IdRef(val_id)],
                ));
                Ok(result_id)
            }
            HIRExpr::Swizzle { base, components } => {
                let base_id = self.generate_expr(base)?;
                let f32_id = self.get_type_id(TypeKey::F32);
                let indices: Vec<_> = components.iter().map(|c| match c {
                    'x' | 'r' | 's' => 0,
                    'y' | 'g' | 't' => 1,
                    'z' | 'b' | 'p' => 2,
                    'w' | 'a' | 'q' => 3,
                    _ => 0,
                }).collect();
                if indices.len() == 1 {
                    let result_id = self.fresh_id();
                    self.add_instr(Instruction::new(
                        Op::CompositeExtract, Some(f32_id), Some(result_id),
                        vec![Operand::IdRef(base_id), Operand::LiteralInt32(indices[0])],
                    ));
                    self.value_types.insert(result_id, Type::F32);
                    Ok(result_id)
                } else {
                    let result_id = self.fresh_id();
                    let vec_ty = match components.len() {
                        2 => self.get_type_id(TypeKey::Vec(2, f32_id)),
                        3 => self.get_type_id(TypeKey::Vec(3, f32_id)),
                        4 => self.get_type_id(TypeKey::Vec(4, f32_id)),
                        _ => return Ok(base_id),
                    };
                    let mut operands = vec![Operand::IdRef(base_id), Operand::IdRef(base_id)];
                    for idx in &indices {
                        operands.push(Operand::LiteralInt32(*idx));
                    }
                    self.add_instr(Instruction::new(Op::VectorShuffle, Some(vec_ty), Some(result_id), operands));
                    self.value_types.insert(result_id, match components.len() {
                        2 => Type::Vec2,
                        3 => Type::Vec3,
                        4 => Type::Vec4,
                        _ => Type::Vec2,
                    });
                    Ok(result_id)
                }
            }
            HIRExpr::VectorConstruct { ty, args } => {
                let arg_ids: Result<Vec<_>, _> = args.iter().map(|a| self.generate_expr(a)).collect();
                let arg_ids = arg_ids?;
                let result_id = self.fresh_id();
                let ty_id = self.type_to_id(ty);
                let operands: Vec<_> = arg_ids.into_iter().map(Operand::IdRef).collect();
                self.add_instr(Instruction::new(Op::CompositeConstruct, Some(ty_id), Some(result_id), operands));
                self.value_types.insert(result_id, ty.clone());
                Ok(result_id)
            }
            HIRExpr::MatrixConstruct { ty, args } => {
                let arg_ids: Result<Vec<_>, _> = args.iter().map(|a| self.generate_expr(a)).collect();
                let arg_ids = arg_ids?;
                let result_id = self.fresh_id();
                let ty_id = self.type_to_id(ty);
                let operands: Vec<_> = arg_ids.into_iter().map(Operand::IdRef).collect();
                self.add_instr(Instruction::new(Op::CompositeConstruct, Some(ty_id), Some(result_id), operands));
                self.value_types.insert(result_id, ty.clone());
                Ok(result_id)
            }
            HIRExpr::Construct { ty, .. } => {
                let result_id = self.fresh_id();
                let ty_id = self.type_to_id(ty);
                self.add_instr(Instruction::new(Op::Undef, Some(ty_id), Some(result_id), vec![]));
                Ok(result_id)
            }
            HIRExpr::Select { cond, then_val, else_val } => {
                let cond_id = self.generate_expr(cond)?;
                let t_id = self.generate_expr(then_val)?;
                let e_id = self.generate_expr(else_val)?;
                let result_id = self.fresh_id();
                let f32_id = self.get_type_id(TypeKey::F32);
                self.add_instr(Instruction::new(
                    Op::Select, Some(f32_id), Some(result_id),
                    vec![Operand::IdRef(cond_id), Operand::IdRef(t_id), Operand::IdRef(e_id)],
                ));
                self.value_types.insert(result_id, Type::F32);
                Ok(result_id)
            }
            HIRExpr::Derivative { kind, val } => {
                let val_id = self.generate_expr(val)?;
                let result_id = self.fresh_id();
                let f32_id = self.get_type_id(TypeKey::F32);
                let op = match kind {
                    DerivativeKind::Dx => Op::DPdx,
                    DerivativeKind::Dy => Op::DPdy,
                    DerivativeKind::Fwidth => Op::Fwidth,
                };
                self.add_instr(Instruction::new(op, Some(f32_id), Some(result_id), vec![Operand::IdRef(val_id)]));
                self.value_types.insert(result_id, Type::F32);
                Ok(result_id)
            }
        }
    }

    fn emit_binary(&mut self, op: BinOp, left: Word, right: Word) -> CompileResult<Word> {
        let f32_id = self.get_type_id(TypeKey::F32);
        let i32_id = self.get_type_id(TypeKey::I32);
        let bool_id = self.get_type_id(TypeKey::Bool);
        let result_id = self.fresh_id();

        let (opcode, ty_id) = match op {
            BinOp::Add => (Op::FAdd, f32_id),
            BinOp::Sub => (Op::FSub, f32_id),
            BinOp::Mul => (Op::FMul, f32_id),
            BinOp::Div => (Op::FDiv, f32_id),
            BinOp::FloorDiv => (Op::SDiv, i32_id),
            BinOp::Mod => (Op::FMod, f32_id),
            BinOp::Pow => (Op::ExtInst, f32_id),
            BinOp::Concat => (Op::FAdd, f32_id),
            BinOp::Eq => (Op::FOrdEqual, bool_id),
            BinOp::NotEq => (Op::FOrdNotEqual, bool_id),
            BinOp::Lt => (Op::FOrdLessThan, bool_id),
            BinOp::Le => (Op::FOrdLessThanEqual, bool_id),
            BinOp::Gt => (Op::FOrdGreaterThan, bool_id),
            BinOp::Ge => (Op::FOrdGreaterThanEqual, bool_id),
            BinOp::And => (Op::LogicalAnd, bool_id),
            BinOp::Or => (Op::LogicalOr, bool_id),
            BinOp::BitAnd => (Op::BitwiseAnd, i32_id),
            BinOp::BitOr => (Op::BitwiseOr, i32_id),
            BinOp::BitXor => (Op::BitwiseXor, i32_id),
            BinOp::ShiftLeft => (Op::ShiftLeftLogical, i32_id),
            BinOp::ShiftRight => (Op::ShiftRightLogical, i32_id),
        };

        self.add_instr(Instruction::new(
            opcode, Some(ty_id), Some(result_id),
            vec![Operand::IdRef(left), Operand::IdRef(right)],
        ));
        self.value_types.insert(result_id, match ty_id {
            x if x == f32_id => Type::F32,
            x if x == i32_id => Type::I32,
            x if x == bool_id => Type::Bool,
            _ => Type::F32,
        });

        Ok(result_id)
    }

    fn emit_unary(&mut self, op: UnOp, val: Word) -> CompileResult<Word> {
        let f32_id = self.get_type_id(TypeKey::F32);
        let bool_id = self.get_type_id(TypeKey::Bool);
        let u32_id = self.get_type_id(TypeKey::U32);
        let result_id = self.fresh_id();

        // UnOp::Len maps to GLSL.std.450 Length (instruction 66) — not a core SPIR-V Op.
        if op == UnOp::Len {
            let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
            self.add_instr(Instruction::new(
                Op::ExtInst, Some(f32_id), Some(result_id),
                vec![
                    Operand::IdRef(glsl_set),
                    Operand::LiteralInt32(66), // GLSLstd450Length
                    Operand::IdRef(val),
                ],
            ));
            self.value_types.insert(result_id, Type::F32);
            return Ok(result_id);
        }

        let (opcode, ty_id) = match op {
            UnOp::Not => (Op::LogicalNot, bool_id),
            UnOp::Neg => (Op::FNegate, f32_id),
            UnOp::BitNot => (Op::Not, u32_id),
            UnOp::Len => unreachable!(),
        };

        self.add_instr(Instruction::new(
            opcode, Some(ty_id), Some(result_id),
            vec![Operand::IdRef(val)],
        ));
        self.value_types.insert(result_id, match ty_id {
            x if x == bool_id => Type::Bool,
            x if x == u32_id => Type::U32,
            _ => Type::F32,
        });

        Ok(result_id)
    }

    fn emit_intrinsic_call(&mut self, func: &str, args: &[Word]) -> CompileResult<Word> {
        let f32_id = self.get_type_id(TypeKey::F32);
        let vec3_id = self.get_type_id(TypeKey::Vec(3, f32_id));
        let vec4_id = self.get_type_id(TypeKey::Vec(4, f32_id));
        let bool_id = self.get_type_id(TypeKey::Bool);

        match func {
            // Identity-style color helpers. These are modeled as pure pass-throughs so the
            // generated SPIR-V stays valid even when the shader uses the helper name as a
            // lightweight post-process step.
            "color.sRGBToLinear" | "color.linearTosRGB" | "color.ACESFilm" | "color.reinhard"
            | "color.uncharted2" | "color.contrast" | "color.saturation" | "color.brightness"
            | "color.hueShift" => {
                return Ok(args.first().copied().unwrap_or(f32_id));
            }

            "max" if args.len() == 2 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(f32_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(40), // GLSLstd450FMax
                        Operand::IdRef(args[0]),
                        Operand::IdRef(args[1]),
                    ],
                ));
                self.value_types.insert(result_id, Type::F32);
                return Ok(result_id);
            }
            "min" if args.len() == 2 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(f32_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(37), // GLSLstd450FMin
                        Operand::IdRef(args[0]),
                        Operand::IdRef(args[1]),
                    ],
                ));
                self.value_types.insert(result_id, Type::F32);
                return Ok(result_id);
            }
            "dot" if args.len() == 2 => {
                let result_id = self.fresh_id();
                self.add_instr(Instruction::new(
                    Op::Dot, Some(f32_id), Some(result_id),
                    vec![Operand::IdRef(args[0]), Operand::IdRef(args[1])],
                ));
                self.value_types.insert(result_id, Type::F32);
                return Ok(result_id);
            }
            "sample" | "sampleLod" | "sampleGrad" | "sampleBias" => {
                if args.len() < 3 {
                    return Ok(vec4_id);
                }

                let texture = args[0];
                let sampler = args[1];
                let coords = args[2];

                let tex_ty = self.value_types.get(&texture).cloned().unwrap_or(Type::Texture2D);
                let inner_ty_id = self.type_to_id(&tex_ty);
                let sampled_image_ty = self.get_type_id(TypeKey::SampledImage(inner_ty_id));
                let sampled_image_id = self.fresh_id();
                self.add_instr(Instruction::new(
                    Op::SampledImage, Some(sampled_image_ty), Some(sampled_image_id),
                    vec![Operand::IdRef(texture), Operand::IdRef(sampler)],
                ));

                let result_id = self.fresh_id();
                self.add_instr(Instruction::new(
                    Op::ImageSampleImplicitLod, Some(vec4_id), Some(result_id),
                    vec![Operand::IdRef(sampled_image_id), Operand::IdRef(coords)],
                ));
                self.value_types.insert(result_id, Type::Vec4);
                return Ok(result_id);
            }
            "cross" if args.len() == 2 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(vec3_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(68), // GLSLstd450Cross
                        Operand::IdRef(args[0]),
                        Operand::IdRef(args[1]),
                    ],
                ));
                self.value_types.insert(result_id, Type::Vec3);
                return Ok(result_id);
            }
            "normalize" if args.len() == 1 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(vec3_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(69), // GLSLstd450Normalize
                        Operand::IdRef(args[0]),
                    ],
                ));
                self.value_types.insert(result_id, Type::Vec3);
                return Ok(result_id);
            }
            "reflect" if args.len() == 2 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(vec3_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(71), // GLSLstd450Reflect
                        Operand::IdRef(args[0]),
                        Operand::IdRef(args[1]),
                    ],
                ));
                self.value_types.insert(result_id, Type::Vec3);
                return Ok(result_id);
            }
            "refract" if args.len() == 3 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(vec3_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(72), // GLSLstd450Refract
                        Operand::IdRef(args[0]),
                        Operand::IdRef(args[1]),
                        Operand::IdRef(args[2]),
                    ],
                ));
                self.value_types.insert(result_id, Type::Vec3);
                return Ok(result_id);
            }
            "faceforward" if args.len() == 3 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(vec3_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(70), // GLSLstd450FaceForward
                        Operand::IdRef(args[0]),
                        Operand::IdRef(args[1]),
                        Operand::IdRef(args[2]),
                    ],
                ));
                self.value_types.insert(result_id, Type::Vec3);
                return Ok(result_id);
            }
            "length" if args.len() == 1 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(f32_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(66), // GLSLstd450Length
                        Operand::IdRef(args[0]),
                    ],
                ));
                self.value_types.insert(result_id, Type::F32);
                return Ok(result_id);
            }
            "distance" if args.len() == 2 => {
                let result_id = self.fresh_id();
                let glsl_set = *self.value_ids.get("GLSL.std.450").expect("GLSL.std.450 not imported");
                self.add_instr(Instruction::new(
                    Op::ExtInst, Some(f32_id), Some(result_id),
                    vec![
                        Operand::IdRef(glsl_set),
                        Operand::LiteralInt32(67), // GLSLstd450Distance
                        Operand::IdRef(args[0]),
                        Operand::IdRef(args[1]),
                    ],
                ));
                self.value_types.insert(result_id, Type::F32);
                return Ok(result_id);
            }
            "select" if args.len() == 3 => {
                let result_id = self.fresh_id();
                self.add_instr(Instruction::new(
                    Op::Select, Some(f32_id), Some(result_id),
                    vec![Operand::IdRef(args[0]), Operand::IdRef(args[1]), Operand::IdRef(args[2])],
                ));
                return Ok(result_id);
            }
            "any" if args.len() == 1 => {
                let result_id = self.fresh_id();
                self.add_instr(Instruction::new(
                    Op::Any, Some(bool_id), Some(result_id),
                    vec![Operand::IdRef(args[0])],
                ));
                return Ok(result_id);
            }
            "all" if args.len() == 1 => {
                let result_id = self.fresh_id();
                self.add_instr(Instruction::new(
                    Op::All, Some(bool_id), Some(result_id),
                    vec![Operand::IdRef(args[0])],
                ));
                return Ok(result_id);
            }
            "vector2.create" | "vector3.create" | "vector4.create"
            | "vector2i.create" | "vector3i.create" | "vector4i.create"
            | "vector2u.create" | "vector3u.create" | "vector4u.create"
            | "bvector2.create" | "bvector3.create" | "bvector4.create" => {
                let i32_id = self.get_type_id(TypeKey::I32);
                let u32_id = self.get_type_id(TypeKey::U32);
                let (n, comp_id, result_ty) = match func {
                    "vector2.create"  => (2u32, f32_id,  Type::Vec2),
                    "vector3.create"  => (3,    f32_id,  Type::Vec3),
                    "vector4.create"  => (4,    f32_id,  Type::Vec4),
                    "vector2i.create" => (2,    i32_id,  Type::Vec2i),
                    "vector3i.create" => (3,    i32_id,  Type::Vec3i),
                    "vector4i.create" => (4,    i32_id,  Type::Vec4i),
                    "vector2u.create" => (2,    u32_id,  Type::Vec2u),
                    "vector3u.create" => (3,    u32_id,  Type::Vec3u),
                    "vector4u.create" => (4,    u32_id,  Type::Vec4u),
                    "bvector2.create" => (2,    bool_id, Type::BVec2),
                    "bvector3.create" => (3,    bool_id, Type::BVec3),
                    "bvector4.create" => (4,    bool_id, Type::BVec4),
                    _ => unreachable!(),
                };
                let ty_id = self.get_type_id(TypeKey::Vec(n, comp_id));
                let result_id = self.fresh_id();
                let zero = self.get_constant_f32(0.0);
                let operands: Vec<Operand> = (0..n as usize)
                    .map(|i| Operand::IdRef(*args.get(i).unwrap_or(&zero)))
                    .collect();
                self.add_instr(Instruction::new(
                    Op::CompositeConstruct, Some(ty_id), Some(result_id), operands,
                ));
                self.value_types.insert(result_id, result_ty);
                return Ok(result_id);
            }
            _ => {
                return Ok(args.first().copied().unwrap_or(f32_id));
            }
        }
    }
    fn std140_align(&self, ty: &Type) -> u32 {
        match ty {
            Type::Bool | Type::I32 | Type::U32 | Type::F32 | Type::Number => 4,
            Type::Vec2 | Type::Vec2i | Type::Vec2u | Type::BVec2 => 8,
            Type::Vec3 | Type::Vec4 | Type::Vec3i | Type::Vec4i | Type::Vec3u | Type::Vec4u | Type::BVec3 | Type::BVec4 => 16,
            Type::Mat2x2 | Type::Mat3x3 | Type::Mat4x4 => 16,
            Type::Array(_, _) | Type::StorageBuffer { .. } | Type::UniformBuffer(_) => 16,
            _ => 4,
        }
    }

    fn std140_size(&self, ty: &Type) -> u32 {
        let align = self.std140_align(ty).max(4);
        let raw = match ty {
            Type::Bool | Type::I32 | Type::U32 | Type::F32 | Type::Number => 4,
            Type::Vec2 | Type::Vec2i | Type::Vec2u | Type::BVec2 => 8,
            Type::Vec3 | Type::Vec4 | Type::Vec3i | Type::Vec4i | Type::Vec3u | Type::Vec4u | Type::BVec3 | Type::BVec4 => 16,
            Type::Mat2x2 => 32,
            Type::Mat3x3 => 48,
            Type::Mat4x4 => 64,
            Type::Array(elem, Some(n)) => align_to(self.std140_size(elem) * *n, align),
            Type::Array(elem, None) => align_to(self.std140_size(elem), align),
            Type::UniformBuffer(fields) => {
                let mut off = 0;
                for f in fields { off = align_to(off, self.std140_align(&f.ty)); off += self.std140_size(&f.ty); }
                off
            }
            Type::StorageBuffer { elem_ty, .. } => self.std140_size(elem_ty),
            _ => 4,
        };
        align_to(raw, align)
    }

    fn get_constant_f32(&mut self, value: f32) -> Word {
        let bits = value.to_bits();
        if let Some(&id) = self.constant_u32_ids.get(&bits) {
            return id;
        }
        let id = self.fresh_id();
        let f32_id = self.get_type_id(TypeKey::F32);
        self.constant_u32_ids.insert(bits, id);
        self.add_global_instr(Instruction::new(
            Op::Constant,
            Some(f32_id),
            Some(id),
            vec![Operand::LiteralInt32(bits)],
        ));
        self.value_types.insert(id, Type::F32);
        id
    }

    fn get_u32_constant_id(&mut self, value: u32) -> Word {
        if let Some(&id) = self.constant_u32_ids.get(&value) {
            return id;
        }
        let id = self.fresh_id();
        let u32_id = self.get_type_id(TypeKey::U32);
        self.constant_u32_ids.insert(value, id);
        self.add_global_instr(Instruction::new(
            Op::Constant,
            Some(u32_id),
            Some(id),
            vec![Operand::LiteralInt32(value)],
        ));
        self.value_types.insert(id, Type::U32);
        id
    }

    fn resolve_struct_member(&self, base: &HIRExpr, field: &str) -> Option<(u32, Type)> {
        match base {
            HIRExpr::Global(name) | HIRExpr::Local(name) => {
                self.binding_types.get(name).or_else(|| self.symbol_types.get(name)).and_then(|ty| match ty {
                    Type::UniformBuffer(fields) => fields.iter().enumerate().find_map(|(i, f)| {
                        if f.name == field {
                            Some((i as u32, f.ty.clone()))
                        } else {
                            None
                        }
                    }),
                    Type::StorageBuffer { elem_ty, .. } => match elem_ty.as_ref() {
                        Type::UniformBuffer(fields) => fields.iter().enumerate().find_map(|(i, f)| {
                            if f.name == field {
                                Some((i as u32, f.ty.clone()))
                            } else {
                                None
                            }
                        }),
                        _ => None,
                    },
                    _ => None,
                })
            }
            _ => None,
        }
    }

    fn build_module(&self) -> Module {
        let mut module = self.module.clone();
        module.types_global_values.splice(0..0, self.decorations.clone());
        // The bound must be greater than all IDs used in the module.
        if let Some(ref mut header) = module.header {
            header.bound = self.next_id.max(1);
        }
        module
    }
}

fn align_to(value: u32, align: u32) -> u32 {
    if align == 0 {
        value
    } else {
        (value + align - 1) / align * align
    }
}

fn stage_to_execution_model(stage: ShaderStage) -> rspirv::spirv::ExecutionModel {
    match stage {
        ShaderStage::Vertex => rspirv::spirv::ExecutionModel::Vertex,
        ShaderStage::Fragment => rspirv::spirv::ExecutionModel::Fragment,
        ShaderStage::Compute => rspirv::spirv::ExecutionModel::GLCompute,
        ShaderStage::PostProcess => rspirv::spirv::ExecutionModel::Fragment,
        ShaderStage::RayGeneration => rspirv::spirv::ExecutionModel::RayGenerationKHR,
        ShaderStage::ClosestHit => rspirv::spirv::ExecutionModel::ClosestHitKHR,
        ShaderStage::Miss => rspirv::spirv::ExecutionModel::MissKHR,
        ShaderStage::Mesh => rspirv::spirv::ExecutionModel::MeshNV,
        ShaderStage::Task => rspirv::spirv::ExecutionModel::TaskNV,
    }
}

fn is_swizzle(s: &str) -> bool {
    s.chars().all(|c| matches!(c, 'x' | 'y' | 'z' | 'w' | 'r' | 'g' | 'b' | 'a' | 's' | 't' | 'p' | 'q'))
        && !s.is_empty()
        && s.len() <= 4
}

impl Default for SpirvGenerator {
    fn default() -> Self {
        Self::new()
    }
}

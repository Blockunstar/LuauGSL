use crate::types::*;
use serde::{Deserialize, Serialize};

/// Reflection data for a compiled shader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaderReflection {
    pub stage: ShaderStage,
    pub entry_point: String,
    pub inputs: Vec<InterfaceVariable>,
    pub outputs: Vec<InterfaceVariable>,
    pub bindings: Vec<BindingReflection>,
    pub push_constants: Vec<FieldDef>,
    pub workgroup_size: Option<[u32; 3]>,
    pub capabilities: Vec<Capability>,
    pub type_map: Vec<TypeReflection>,
}

/// An interface variable (input/output).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InterfaceVariable {
    pub name: String,
    pub ty: Type,
    pub location: Option<u32>,
    pub builtin: Option<String>,
    pub interpolate: Option<String>,
}

/// Reflection data for a resource binding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingReflection {
    pub set: u32,
    pub binding: u32,
    pub name: String,
    pub ty: Type,
    pub stages: Vec<ShaderStage>,
}

/// Reflection data for a type used in the shader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeReflection {
    pub name: String,
    pub ty: Type,
    pub size: u32,
    pub alignment: u32,
}

/// A compiled shader artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub magic: [u8; 4],
    pub version: u32,
    pub stage: ShaderStage,
    pub spirv_bytecode: Vec<u32>,
    pub reflection: ShaderReflection,
    pub capability_manifest: Vec<Capability>,
    pub source_hash: Vec<u8>,
    pub body_hash: Vec<u8>,
    pub signature: Option<Vec<u8>>,
    pub public_key_hash: Option<Vec<u8>>,
}

impl Artifact {
    pub const MAGIC: [u8; 4] = *b"LGSA";
    pub const VERSION: u32 = 1;

    pub fn new(
        stage: ShaderStage,
        spirv_bytecode: Vec<u32>,
        reflection: ShaderReflection,
        capabilities: Vec<Capability>,
        source_hash: Vec<u8>,
    ) -> Self {
        Artifact {
            magic: Self::MAGIC,
            version: Self::VERSION,
            stage,
            spirv_bytecode,
            reflection,
            capability_manifest: capabilities.clone(),
            source_hash: source_hash.clone(),
            body_hash: Vec::new(), // Computed during signing
            signature: None,
            public_key_hash: None,
        }
    }
}

/// Pipeline configuration for shader dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineConfig {
    pub stage: ShaderStage,
    pub entry_point: String,
    pub workgroup_size: Option<[u32; 3]>,
    pub bindings: Vec<BindingConfig>,
}

/// A runtime binding configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BindingConfig {
    pub set: u32,
    pub binding: u32,
    pub resource: BindingResource,
}

/// Types of resources that can be bound.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BindingResource {
    Buffer { handle: u64, size: u64, offset: u64 },
    Texture { handle: u64 },
    Sampler { handle: u64 },
    PushConstants { data: Vec<u8> },
}

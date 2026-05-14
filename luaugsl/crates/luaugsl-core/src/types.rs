use serde::{Deserialize, Serialize};
use std::fmt;

/// Access modifier for storage buffers and struct fields.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Access {
    Read,
    Write,
    ReadWrite,
}

/// A field definition for tables, uniform buffers, etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub name: String,
    pub ty: Type,
    pub access: Option<Access>,
    pub offset: Option<u32>,
    pub align: Option<u32>,
    pub location: Option<u32>,
    pub interpolate: Option<String>,
    pub builtin: Option<String>,
}

/// A function/parameter definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub builtin: Option<String>,
    pub interpolate: Option<String>,
    pub location: Option<u32>,
}

/// Shader stage kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
    PostProcess,
    RayGeneration,
    ClosestHit,
    Miss,
    Mesh,
    Task,
}

impl fmt::Display for ShaderStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShaderStage::Vertex => write!(f, "vertex"),
            ShaderStage::Fragment => write!(f, "fragment"),
            ShaderStage::Compute => write!(f, "compute"),
            ShaderStage::PostProcess => write!(f, "postprocess"),
            ShaderStage::RayGeneration => write!(f, "raygeneration"),
            ShaderStage::ClosestHit => write!(f, "closesthit"),
            ShaderStage::Miss => write!(f, "miss"),
            ShaderStage::Mesh => write!(f, "mesh"),
            ShaderStage::Task => write!(f, "task"),
        }
    }
}

impl std::str::FromStr for ShaderStage {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "vertex" => Ok(ShaderStage::Vertex),
            "fragment" => Ok(ShaderStage::Fragment),
            "compute" => Ok(ShaderStage::Compute),
            "postprocess" => Ok(ShaderStage::PostProcess),
            "raygeneration" => Ok(ShaderStage::RayGeneration),
            "closesthit" => Ok(ShaderStage::ClosestHit),
            "miss" => Ok(ShaderStage::Miss),
            "mesh" => Ok(ShaderStage::Mesh),
            "task" => Ok(ShaderStage::Task),
            _ => Err(format!("unknown shader stage: {}", s)),
        }
    }
}

/// The complete type system for LuauGSL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Type {
    // Primitives
    Nil,
    Bool,
    Number,
    String,
    F32,
    I32,
    U32,

    // Vector types (float)
    Vec2,
    Vec3,
    Vec4,
    // Vector types (signed int)
    Vec2i,
    Vec3i,
    Vec4i,
    // Vector types (unsigned int)
    Vec2u,
    Vec3u,
    Vec4u,
    // Vector types (bool)
    BVec2,
    BVec3,
    BVec4,

    // Matrix types
    Mat2x2,
    Mat3x3,
    Mat4x4,

    // Texture types
    Texture2D,
    Texture3D,
    TextureCube,
    Texture2DArray,
    TextureCubeArray,
    StorageImage { format: String },

    // GPU resource types
    StorageBuffer { elem_ty: Box<Type>, access: Access },
    Sampler,
    UniformBuffer(Vec<FieldDef>),

    // Composite types
    Array(Box<Type>, Option<u32>),
    Function(Vec<Type>, Box<Type>),
    Table(Vec<FieldDef>, Option<(Box<Type>, Box<Type>)>),
    Union(Vec<Type>),
    Intersection(Vec<Type>),
    Generic(String, Vec<Type>),
    Optional(Box<Type>),

    // Special types
    Unknown,
    Error,
    Never,
}

impl Type {
    /// Check if this type is a vector type (any component type).
    pub fn is_vector(&self) -> bool {
        matches!(
            self,
            Type::Vec2 | Type::Vec3 | Type::Vec4
                | Type::Vec2i | Type::Vec3i | Type::Vec4i
                | Type::Vec2u | Type::Vec3u | Type::Vec4u
                | Type::BVec2 | Type::BVec3 | Type::BVec4
        )
    }

    /// Check if this is a float vector type.
    pub fn is_float_vector(&self) -> bool {
        matches!(self, Type::Vec2 | Type::Vec3 | Type::Vec4)
    }

    /// Check if this is a numeric scalar type.
    pub fn is_numeric_scalar(&self) -> bool {
        matches!(self, Type::F32 | Type::I32 | Type::U32 | Type::Number)
    }

    /// Check if this type is a matrix type.
    pub fn is_matrix(&self) -> bool {
        matches!(self, Type::Mat2x2 | Type::Mat3x3 | Type::Mat4x4)
    }

    /// Check if this type is a texture type.
    pub fn is_texture(&self) -> bool {
        matches!(
            self,
            Type::Texture2D | Type::Texture3D | Type::TextureCube
                | Type::Texture2DArray | Type::TextureCubeArray | Type::StorageImage { .. }
        )
    }

    /// Get the scalar component type of a vector.
    pub fn vector_scalar_type(&self) -> Option<Type> {
        match self {
            Type::Vec2 | Type::Vec3 | Type::Vec4 => Some(Type::F32),
            Type::Vec2i | Type::Vec3i | Type::Vec4i => Some(Type::I32),
            Type::Vec2u | Type::Vec3u | Type::Vec4u => Some(Type::U32),
            Type::BVec2 | Type::BVec3 | Type::BVec4 => Some(Type::Bool),
            _ => None,
        }
    }

    /// Get the number of components in a vector type.
    pub fn vector_components(&self) -> Option<u32> {
        match self {
            Type::Vec2 | Type::Vec2i | Type::Vec2u | Type::BVec2 => Some(2),
            Type::Vec3 | Type::Vec3i | Type::Vec3u | Type::BVec3 => Some(3),
            Type::Vec4 | Type::Vec4i | Type::Vec4u | Type::BVec4 => Some(4),
            _ => None,
        }
    }

    /// Check if this type is valid in GPU/shader contexts (no f64, no host-only types).
    pub fn is_gpu_valid(&self) -> bool {
        match self {
            Type::F32 | Type::I32 | Type::U32 | Type::Bool | Type::Nil => true,
            Type::Number => true, // Resolved to f32 in shader contexts
            Type::Vec2 | Type::Vec3 | Type::Vec4 => true,
            Type::Vec2i | Type::Vec3i | Type::Vec4i => true,
            Type::Vec2u | Type::Vec3u | Type::Vec4u => true,
            Type::BVec2 | Type::BVec3 | Type::BVec4 => true,
            Type::Mat2x2 | Type::Mat3x3 | Type::Mat4x4 => true,
            Type::Texture2D | Type::Texture3D | Type::TextureCube
                | Type::Texture2DArray | Type::TextureCubeArray | Type::StorageImage { .. } => true,
            Type::StorageBuffer { elem_ty, .. } => elem_ty.is_gpu_valid(),
            Type::Sampler => true,
            Type::UniformBuffer(fields) => fields.iter().all(|f| f.ty.is_gpu_valid()),
            Type::Array(ty, _) => ty.is_gpu_valid(),
            Type::Function(params, ret) => {
                params.iter().all(|p| p.is_gpu_valid()) && ret.is_gpu_valid()
            }
            Type::Table(fields, _) => fields.iter().all(|f| f.ty.is_gpu_valid()),
            Type::Union(tys) | Type::Intersection(tys) => tys.iter().all(|t| t.is_gpu_valid()),
            Type::Generic(_, args) => args.iter().all(|a| a.is_gpu_valid()),
            Type::Optional(ty) => ty.is_gpu_valid(),
            Type::Unknown | Type::Error | Type::Never => true,
            Type::String => true, // Compile-time only in shaders
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Nil => write!(f, "nil"),
            Type::Bool => write!(f, "boolean"),
            Type::Number => write!(f, "number"),
            Type::String => write!(f, "string"),
            Type::F32 => write!(f, "f32"),
            Type::I32 => write!(f, "i32"),
            Type::U32 => write!(f, "u32"),
            Type::Vec2 => write!(f, "vector2"),
            Type::Vec3 => write!(f, "vector3"),
            Type::Vec4 => write!(f, "vector4"),
            Type::Vec2i => write!(f, "vector2i"),
            Type::Vec3i => write!(f, "vector3i"),
            Type::Vec4i => write!(f, "vector4i"),
            Type::Vec2u => write!(f, "vector2u"),
            Type::Vec3u => write!(f, "vector3u"),
            Type::Vec4u => write!(f, "vector4u"),
            Type::BVec2 => write!(f, "bvector2"),
            Type::BVec3 => write!(f, "bvector3"),
            Type::BVec4 => write!(f, "bvector4"),
            Type::Mat2x2 => write!(f, "mat2x2"),
            Type::Mat3x3 => write!(f, "mat3x3"),
            Type::Mat4x4 => write!(f, "mat4x4"),
            Type::Texture2D => write!(f, "texture2d"),
            Type::Texture3D => write!(f, "texture3d"),
            Type::TextureCube => write!(f, "textureCube"),
            Type::Texture2DArray => write!(f, "texture2dArray"),
            Type::TextureCubeArray => write!(f, "textureCubeArray"),
            Type::StorageImage { format } => write!(f, "storageImage<{}>", format),
            Type::StorageBuffer { elem_ty, access } => {
                let access_str = match access {
                    Access::Read => "read ",
                    Access::Write => "write ",
                    Access::ReadWrite => "",
                };
                write!(f, "storageBuffer<{}{}>", access_str, elem_ty)
            }
            Type::Sampler => write!(f, "sampler"),
            Type::UniformBuffer(fields) => {
                write!(f, "uniform {{")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", field.name, field.ty)?;
                }
                write!(f, "}}")
            }
            Type::Array(ty, Some(n)) => write!(f, "{{{}, {}}}", ty, n),
            Type::Array(ty, None) => write!(f, "{{{}}}", ty),
            Type::Function(params, ret) => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
            Type::Table(fields, None) => {
                write!(f, "{{")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", field.name, field.ty)?;
                }
                write!(f, "}}")
            }
            Type::Table(fields, Some((key_ty, val_ty))) if fields.is_empty() => {
                write!(f, "{{[{}]: {}}}", key_ty, val_ty)
            }
            Type::Table(fields, Some((key_ty, val_ty))) => {
                write!(f, "{{")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}: {}", field.name, field.ty)?;
                }
                write!(f, "; [{}]: {}}}", key_ty, val_ty)
            }
            Type::Union(tys) => {
                for (i, t) in tys.iter().enumerate() {
                    if i > 0 { write!(f, " | ")?; }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            Type::Intersection(tys) => {
                for (i, t) in tys.iter().enumerate() {
                    if i > 0 { write!(f, " & ")?; }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            Type::Generic(name, args) => {
                write!(f, "{}", name)?;
                if !args.is_empty() {
                    write!(f, "<")?;
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{}", a)?;
                    }
                    write!(f, ">")?;
                }
                Ok(())
            }
            Type::Optional(ty) => write!(f, "{}?", ty),
            Type::Unknown => write!(f, "unknown"),
            Type::Error => write!(f, "<error>"),
            Type::Never => write!(f, "never"),
        }
    }
}

/// A binding declaration for shader resources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub set: u32,
    pub binding: u32,
    pub name: String,
    pub ty: Type,
    pub stage: Option<ShaderStage>,
}

/// Capability declaration for a shader.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    TextureSample,
    StorageRead,
    StorageWrite,
    AtomicOps,
    Derivatives,
    SubgroupOps,
    RayTracing,
    MeshShader,
    Discard,
    ImageReadWrite,
    PushConstant,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Capability::TextureSample => write!(f, "textureSample"),
            Capability::StorageRead => write!(f, "storageRead"),
            Capability::StorageWrite => write!(f, "storageWrite"),
            Capability::AtomicOps => write!(f, "atomicOps"),
            Capability::Derivatives => write!(f, "derivatives"),
            Capability::SubgroupOps => write!(f, "subgroupOps"),
            Capability::RayTracing => write!(f, "rayTracing"),
            Capability::MeshShader => write!(f, "meshShader"),
            Capability::Discard => write!(f, "discard"),
            Capability::ImageReadWrite => write!(f, "imageReadWrite"),
            Capability::PushConstant => write!(f, "pushConstant"),
        }
    }
}

/// Intrinsic capability requirements per shader stage.
pub fn stage_capabilities(stage: ShaderStage) -> Vec<Capability> {
    match stage {
        ShaderStage::Vertex => vec![
            Capability::TextureSample,
            Capability::PushConstant,
        ],
        ShaderStage::Fragment => vec![
            Capability::TextureSample,
            Capability::StorageRead,
            Capability::Derivatives,
            Capability::Discard,
            Capability::PushConstant,
        ],
        ShaderStage::Compute => vec![
            Capability::TextureSample,
            Capability::StorageRead,
            Capability::StorageWrite,
            Capability::AtomicOps,
            Capability::ImageReadWrite,
            Capability::PushConstant,
            Capability::SubgroupOps,
        ],
        ShaderStage::PostProcess => vec![
            Capability::TextureSample,
            Capability::StorageWrite,
            Capability::ImageReadWrite,
        ],
        ShaderStage::RayGeneration => vec![
            Capability::RayTracing,
            Capability::TextureSample,
        ],
        ShaderStage::ClosestHit => vec![
            Capability::RayTracing,
            Capability::TextureSample,
        ],
        ShaderStage::Miss => vec![
            Capability::RayTracing,
            Capability::TextureSample,
        ],
        ShaderStage::Mesh => vec![
            Capability::MeshShader,
            Capability::TextureSample,
            Capability::StorageRead,
        ],
        ShaderStage::Task => vec![
            Capability::MeshShader,
            Capability::StorageRead,
        ],
    }
}

/// Safety limits for DoS prevention.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SafetyLimits {
    pub max_instructions: u32,
    pub max_loop_nesting: u32,
    pub max_call_depth: u32,
    pub max_live_variables: u32,
    pub max_workgroup_size: u32,
    pub max_texture_samples: u32,
    pub max_bindings: u32,
}

impl Default for SafetyLimits {
    fn default() -> Self {
        SafetyLimits {
            max_instructions: 100_000,
            max_loop_nesting: 8,
            max_call_depth: 32,
            max_live_variables: 1024,
            max_workgroup_size: 1024,
            max_texture_samples: 256,
            max_bindings: 64,
        }
    }
}

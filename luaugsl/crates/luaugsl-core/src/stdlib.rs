use crate::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// An intrinsic function entry in the standard library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Intrinsic {
    pub name: String,
    pub namespace: Option<String>,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub is_overload: bool,
    pub min_stage: Option<ShaderStage>,
    pub effect_free: bool,
}

/// The complete standard library definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdLib {
    pub intrinsics: HashMap<String, Vec<Intrinsic>>,
    pub namespaces: HashMap<String, HashMap<String, Vec<Intrinsic>>>,
}

impl Default for StdLib {
    fn default() -> Self {
        Self::new()
    }
}

impl StdLib {
    pub fn new() -> Self {
        let mut lib = StdLib {
            intrinsics: HashMap::new(),
            namespaces: HashMap::new(),
        };
        lib.register_math();
        lib.register_vector();
        lib.register_matrix();
        lib.register_texture();
        lib.register_buffer();
        lib.register_noise();
        lib.register_color();
        lib.register_geo();
        lib.register_bit32();
        lib.register_utility();
        lib
    }

    /// Look up a function by name (non-namespaced).
    pub fn lookup(&self, name: &str) -> Option<&Vec<Intrinsic>> {
        self.intrinsics.get(name)
    }

    /// Look up a namespaced function: `noise.perlin`, `bit32.band`, etc.
    pub fn lookup_namespaced(&self, namespace: &str, name: &str) -> Option<&Vec<Intrinsic>> {
        self.namespaces.get(namespace)?.get(name)
    }

    fn add(&mut self, name: impl Into<String>, params: Vec<(String, Type)>, return_type: Type) {
        let name = name.into();
        let intrinsic = Intrinsic {
            name: name.clone(),
            namespace: None,
            params,
            return_type,
            is_overload: false,
            min_stage: None,
            effect_free: true,
        };
        self.intrinsics.entry(name).or_default().push(intrinsic);
    }

    fn add_ns(
        &mut self,
        ns: impl Into<String>,
        name: impl Into<String>,
        params: Vec<(String, Type)>,
        return_type: Type,
    ) {
        let ns = ns.into();
        let name = name.into();
        let intrinsic = Intrinsic {
            name: name.clone(),
            namespace: Some(ns.clone()),
            params,
            return_type,
            is_overload: false,
            min_stage: None,
            effect_free: true,
        };
        self.namespaces
            .entry(ns)
            .or_default()
            .entry(name)
            .or_default()
            .push(intrinsic);
    }

    fn add_with_stage(
        &mut self,
        name: impl Into<String>,
        params: Vec<(String, Type)>,
        return_type: Type,
        stage: ShaderStage,
    ) {
        let name = name.into();
        let intrinsic = Intrinsic {
            name: name.clone(),
            namespace: None,
            params,
            return_type,
            is_overload: false,
            min_stage: Some(stage),
            effect_free: true,
        };
        self.intrinsics.entry(name).or_default().push(intrinsic);
    }

    fn f32_ty() -> Type { Type::F32 }
    fn i32_ty() -> Type { Type::I32 }
    fn u32_ty() -> Type { Type::U32 }
    fn bool_ty() -> Type { Type::Bool }
    fn vec2_ty() -> Type { Type::Vec2 }
    fn vec3_ty() -> Type { Type::Vec3 }
    fn vec4_ty() -> Type { Type::Vec4 }
    fn vec2i_ty() -> Type { Type::Vec2i }
    fn vec3i_ty() -> Type { Type::Vec3i }
    fn vec4i_ty() -> Type { Type::Vec4i }
    fn vec2u_ty() -> Type { Type::Vec2u }
    fn vec3u_ty() -> Type { Type::Vec3u }
    fn vec4u_ty() -> Type { Type::Vec4u }
    fn bvec2_ty() -> Type { Type::BVec2 }
    fn bvec3_ty() -> Type { Type::BVec3 }
    fn bvec4_ty() -> Type { Type::BVec4 }
    fn mat2x2_ty() -> Type { Type::Mat2x2 }
    fn mat3x3_ty() -> Type { Type::Mat3x3 }
    fn mat4x4_ty() -> Type { Type::Mat4x4 }
    fn tex2d_ty() -> Type { Type::Texture2D }
    fn tex3d_ty() -> Type { Type::Texture3D }
    fn texcube_ty() -> Type { Type::TextureCube }
    fn sampler_ty() -> Type { Type::Sampler }

    fn register_math(&mut self) {
        let f = Self::f32_ty;
        let _v2 = Self::vec2_ty;
        let v3 = Self::vec3_ty;
        let v4 = Self::vec4_ty;
        let _f_or_v = || f(); // scalar or vector overloads handled by typeck

        macro_rules! math1 {
            ($($name:ident),*) => {
                $(self.add(stringify!($name), vec![("x".into(), f())], f());)*
            };
        }
        macro_rules! math2 {
            ($($name:ident),*) => {
                $(self.add(stringify!($name), vec![("a".into(), f()), ("b".into(), f())], f());)*
            };
        }
        macro_rules! math3 {
            ($($name:ident),*) => {
                $(self.add(stringify!($name), vec![("a".into(), f()), ("b".into(), f()), ("c".into(), f())], f());)*
            };
        }

        math1!(sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, sqrt, rsqrt, exp, exp2, log, log2, abs, sign, floor, ceil, round, trunc, fract);
        math2!(pow, mod2, min, max, atan2);
        math3!(clamp, lerp, smoothstep, fma);
        self.add("mix", vec![("a".into(), f()), ("b".into(), f()), ("t".into(), f())], f());

        // Override `mod` (Rust keyword conflict) — use mod2 and handle in codegen
        // Actually, the real name is `mod` in LuauGSL — we'll map it in codegen
        self.intrinsics.remove("mod2");
        self.add("mod", vec![("x".into(), f()), ("y".into(), f())], f());

        math1!(step);
        self.add("step", vec![("edge".into(), f()), ("x".into(), f())], f());

        // Geometric functions (also available at top level)
        self.add("length", vec![("v".into(), v3())], f());
        self.add("distance", vec![("a".into(), v3()), ("b".into(), v3())], f());
        self.add("dot", vec![("a".into(), v3()), ("b".into(), v3())], f());
        self.add("cross", vec![("a".into(), v3()), ("b".into(), v3())], v3());
        self.add("normalize", vec![("v".into(), v3())], v3());
        self.add("reflect", vec![("i".into(), v3()), ("n".into(), v3())], v3());
        self.add("refract", vec![("i".into(), v3()), ("n".into(), v3()), ("eta".into(), f())], v3());
        self.add("faceforward", vec![("n".into(), v3()), ("i".into(), v3()), ("nref".into(), v3())], v3());

        self.add("transpose", vec![("m".into(), Self::mat4x4_ty())], Self::mat4x4_ty());
        self.add("inverse", vec![("m".into(), Self::mat4x4_ty())], Self::mat4x4_ty());
        self.add("determinant", vec![("m".into(), Self::mat4x4_ty())], f());

        // Matrix constructors
        self.add("mat2x2.create", vec![("m00".into(), f()), ("m01".into(), f()), ("m10".into(), f()), ("m11".into(), f())], Self::mat2x2_ty());
        self.add("mat3x3.create", vec![("m00".into(), f()), ("m01".into(), f()), ("m02".into(), f()),
                                         ("m10".into(), f()), ("m11".into(), f()), ("m12".into(), f()),
                                         ("m20".into(), f()), ("m21".into(), f()), ("m22".into(), f())], Self::mat3x3_ty());
        self.add("mat4x4.create", vec![("m00".into(), f()), ("m01".into(), f()), ("m02".into(), f()), ("m03".into(), f()),
                                          ("m10".into(), f()), ("m11".into(), f()), ("m12".into(), f()), ("m13".into(), f()),
                                          ("m20".into(), f()), ("m21".into(), f()), ("m22".into(), f()), ("m23".into(), f()),
                                          ("m30".into(), f()), ("m31".into(), f()), ("m32".into(), f()), ("m33".into(), f())], Self::mat4x4_ty());

        self.add("mat4x4.createTRS", vec![("translation".into(), v3()), ("rotation".into(), v4()), ("scale".into(), v3())], Self::mat4x4_ty());
        self.add("mat4x4.createTranslation", vec![("t".into(), v3())], Self::mat4x4_ty());
        self.add("mat4x4.createRotation", vec![("axis".into(), v3()), ("angle".into(), f())], Self::mat4x4_ty());
        self.add("mat4x4.createScale", vec![("s".into(), v3())], Self::mat4x4_ty());
        self.add("mat4x4.createLookAt", vec![("eye".into(), v3()), ("center".into(), v3()), ("up".into(), v3())], Self::mat4x4_ty());
        self.add("mat4x4.createPerspective", vec![("fov".into(), f()), ("aspect".into(), f()), ("near".into(), f()), ("far".into(), f())], Self::mat4x4_ty());
        self.add("mat4x4.createOrthographic", vec![("left".into(), f()), ("right".into(), f()), ("bottom".into(), f()), ("top".into(), f()), ("near".into(), f()), ("far".into(), f())], Self::mat4x4_ty());
    }

    fn register_vector(&mut self) {
        let f = Self::f32_ty;
        let i = Self::i32_ty;
        let u = Self::u32_ty;
        let b = Self::bool_ty;

        // Float vector constructors
        self.add("vector2.create", vec![("x".into(), f()), ("y".into(), f())], Self::vec2_ty());
        self.add("vector3.create", vec![("x".into(), f()), ("y".into(), f()), ("z".into(), f())], Self::vec3_ty());
        self.add("vector4.create", vec![("x".into(), f()), ("y".into(), f()), ("z".into(), f()), ("w".into(), f())], Self::vec4_ty());

        // Integer vector constructors
        self.add("vector2i.create", vec![("x".into(), i()), ("y".into(), i())], Self::vec2i_ty());
        self.add("vector3i.create", vec![("x".into(), i()), ("y".into(), i()), ("z".into(), i())], Self::vec3i_ty());
        self.add("vector4i.create", vec![("x".into(), i()), ("y".into(), i()), ("z".into(), i()), ("w".into(), i())], Self::vec4i_ty());

        // Unsigned integer vector constructors
        self.add("vector2u.create", vec![("x".into(), u()), ("y".into(), u())], Self::vec2u_ty());
        self.add("vector3u.create", vec![("x".into(), u()), ("y".into(), u()), ("z".into(), u())], Self::vec3u_ty());
        self.add("vector4u.create", vec![("x".into(), u()), ("y".into(), u()), ("z".into(), u()), ("w".into(), u())], Self::vec4u_ty());

        // Boolean vector constructors
        self.add("bvector2.create", vec![("x".into(), b()), ("y".into(), b())], Self::bvec2_ty());
        self.add("bvector3.create", vec![("x".into(), b()), ("y".into(), b()), ("z".into(), b())], Self::bvec3_ty());
        self.add("bvector4.create", vec![("x".into(), b()), ("y".into(), b()), ("z".into(), b()), ("w".into(), b())], Self::bvec4_ty());

        // Conversion constructors
        self.add("vector3i.createFromFloat", vec![("v".into(), Self::vec3_ty())], Self::vec3i_ty());
        self.add("vector3u.createFromFloat", vec![("v".into(), Self::vec3_ty())], Self::vec3u_ty());
    }

    fn register_matrix(&mut self) {
        // Already registered in math section
    }

    fn register_texture(&mut self) {
        let f = Self::f32_ty;
        let v2 = Self::vec2_ty;
        let _v3 = Self::vec3_ty;
        let v4 = Self::vec4_ty;
        let v2i = Self::vec2i_ty;
        let t2d = Self::tex2d_ty;
        let _t3d = Self::tex3d_ty;
        let _tcube = Self::texcube_ty;
        let samp = Self::sampler_ty;

        self.add("sample", vec![("texture".into(), t2d()), ("sampler".into(), samp()), ("uv".into(), v2())], v4());
        self.add("sampleLod", vec![("texture".into(), t2d()), ("sampler".into(), samp()), ("uv".into(), v2()), ("lod".into(), f())], v4());
        self.add("sampleGrad", vec![("texture".into(), t2d()), ("sampler".into(), samp()), ("uv".into(), v2()), ("ddx".into(), v2()), ("ddy".into(), v2())], v4());
        self.add("sampleBias", vec![("texture".into(), t2d()), ("sampler".into(), samp()), ("uv".into(), v2()), ("bias".into(), f())], v4());
        self.add("sampleCompare", vec![("texture".into(), t2d()), ("sampler".into(), samp()), ("uv".into(), v2()), ("compare".into(), f())], f());

        self.add("textureFetch", vec![("texture".into(), t2d()), ("coord".into(), v2i()), ("lod".into(), Self::i32_ty())], v4());
        self.add("imageWrite", vec![("image".into(), Self::f32_ty()), ("coord".into(), v2i()), ("value".into(), v4())], Type::Nil);
        self.add("imageRead", vec![("image".into(), Self::f32_ty()), ("coord".into(), v2i())], v4());
        self.add("imageDimensions", vec![("image".into(), Self::f32_ty())], Self::vec2i_ty());

        self.add("getDimensions", vec![("texture".into(), t2d())], Self::vec2i_ty());
        self.add("getMipLevels", vec![("texture".into(), t2d())], Self::u32_ty());
    }

    fn register_buffer(&mut self) {
        let u = Self::u32_ty;
        let t = Type::Generic("T".into(), vec![]);

        self.add("bufferLength", vec![("buffer".into(), t.clone())], u());
        self.add("bufferLoad", vec![("buffer".into(), t.clone()), ("index".into(), u())], t.clone());
        self.add("bufferStore", vec![("buffer".into(), t.clone()), ("index".into(), u()), ("value".into(), t.clone())], Type::Nil);

        // Atomic operations (compute only)
        let i = Self::i32_ty;
        macro_rules! atomic {
            ($($name:ident),*) => {
                $(self.add_with_stage(stringify!($name), vec![("buffer".into(), t.clone()), ("index".into(), u()), ("value".into(), i())], i(), ShaderStage::Compute);)*
            };
        }
        atomic!(atomicAdd, atomicSub, atomicMin, atomicMax, atomicAnd, atomicOr, atomicXor, atomicExchange);

        self.add_with_stage("atomicCompareExchange", vec![
            ("buffer".into(), t.clone()), ("index".into(), u()), ("compare".into(), i()), ("value".into(), i())
        ], i(), ShaderStage::Compute);
    }

    fn register_noise(&mut self) {
        let f = Self::f32_ty;
        let v2 = Self::vec2_ty;
        let v3 = Self::vec3_ty;

        self.add_ns("noise", "perlin", vec![("coord".into(), v2())], f());
        self.add_ns("noise", "simplex", vec![("coord".into(), v2())], f());
        self.add_ns("noise", "worley", vec![("coord".into(), v2())], v2());
        self.add_ns("noise", "worleyF1", vec![("coord".into(), v2())], f());
        self.add_ns("noise", "worleyF2", vec![("coord".into(), v2())], f());
        self.add_ns("noise", "fbm", vec![("coord".into(), v3()), ("octaves".into(), Self::i32_ty()), ("lacunarity".into(), f()), ("persistence".into(), f()), ("scale".into(), f())], f());
        self.add_ns("noise", "turbulence", vec![("coord".into(), v3()), ("octaves".into(), Self::i32_ty()), ("lacunarity".into(), f()), ("persistence".into(), f()), ("scale".into(), f())], f());
        self.add_ns("noise", "ridge", vec![("coord".into(), v3()), ("octaves".into(), Self::i32_ty()), ("lacunarity".into(), f()), ("persistence".into(), f()), ("scale".into(), f())], f());
        self.add_ns("noise", "value", vec![("coord".into(), v2())], f());
        self.add_ns("noise", "gradient", vec![("coord".into(), v2())], v3());
        self.add_ns("noise", "white", vec![], f());

        self.add_ns("noise", "hash", vec![("seed".into(), Self::u32_ty())], Self::u32_ty());
        self.add_ns("noise", "hashf", vec![("seed".into(), Self::u32_ty())], f());
    }

    fn register_color(&mut self) {
        let v3 = Self::vec3_ty;
        let v4 = Self::vec4_ty;
        let f = Self::f32_ty;

        self.add_ns("color", "sRGBToLinear", vec![("c".into(), v3())], v3());
        self.add_ns("color", "linearTosRGB", vec![("c".into(), v3())], v3());
        self.add_ns("color", "ACESFilm", vec![("c".into(), v3())], v3());
        self.add_ns("color", "reinhard", vec![("c".into(), v3())], v3());
        self.add_ns("color", "uncharted2", vec![("c".into(), v3())], v3());
        self.add_ns("color", "luminance", vec![("c".into(), v3())], f());
        self.add_ns("color", "contrast", vec![("c".into(), v3()), ("amount".into(), f())], v3());
        self.add_ns("color", "saturation", vec![("c".into(), v3()), ("amount".into(), f())], v3());
        self.add_ns("color", "brightness", vec![("c".into(), v3()), ("amount".into(), f())], v3());
        self.add_ns("color", "hueShift", vec![("c".into(), v3()), ("degrees".into(), f())], v3());
        self.add_ns("color", "blend", vec![("mode".into(), Type::String), ("a".into(), v4()), ("b".into(), v4())], v4());
    }

    fn register_geo(&mut self) {
        let v3 = Self::vec3_ty;
        let v2 = Self::vec2_ty;
        let b = Self::bool_ty;
        let f = Self::f32_ty;

        self.add_ns("geo", "raySphere", vec![("rayOrigin".into(), v3()), ("rayDir".into(), v3()), ("sphereCenter".into(), v3()), ("sphereRadius".into(), f())], v3());
        self.add_ns("geo", "rayAABB", vec![("rayOrigin".into(), v3()), ("rayDir".into(), v3()), ("aabbMin".into(), v3()), ("aabbMax".into(), v3())], v2());
        self.add_ns("geo", "rayTriangle", vec![("rayOrigin".into(), v3()), ("rayDir".into(), v3()), ("v0".into(), v3()), ("v1".into(), v3()), ("v2".into(), v3())], v3());
        self.add_ns("geo", "rayPlane", vec![("rayOrigin".into(), v3()), ("rayDir".into(), v3()), ("planePoint".into(), v3()), ("planeNormal".into(), v3())], v2());
        self.add_ns("geo", "barycentric", vec![("point".into(), v3()), ("v0".into(), v3()), ("v1".into(), v3()), ("v2".into(), v3())], v3());
        self.add_ns("geo", "pointInTriangle", vec![("p".into(), v2()), ("v0".into(), v2()), ("v1".into(), v2()), ("v2".into(), v2())], b());
        self.add_ns("geo", "distancePointToLine", vec![("point".into(), v3()), ("linePoint".into(), v3()), ("lineDir".into(), v3())], f());
        self.add_ns("geo", "distancePointToSegment", vec![("point".into(), v3()), ("segA".into(), v3()), ("segB".into(), v3())], f());
        self.add_ns("geo", "closestPointOnSegment", vec![("point".into(), v3()), ("segA".into(), v3()), ("segB".into(), v3())], v3());
        self.add_ns("geo", "sphereAABB", vec![("center".into(), v3()), ("radius".into(), f()), ("aabbMin".into(), v3()), ("aabbMax".into(), v3())], b());
        self.add_ns("geo", "aabbAABB", vec![("aMin".into(), v3()), ("aMax".into(), v3()), ("bMin".into(), v3()), ("bMax".into(), v3())], b());
        self.add_ns("geo", "sampleHemisphereCosine", vec![("normal".into(), v3()), ("u".into(), f()), ("v".into(), f())], v3());
        self.add_ns("geo", "sampleHemisphereUniform", vec![("u".into(), f()), ("v".into(), f())], v3());
        self.add_ns("geo", "sampleDiskConcentric", vec![("u".into(), f()), ("v".into(), f())], v2());
        self.add_ns("geo", "sampleSphereUniform", vec![("u".into(), f()), ("v".into(), f())], v3());
    }

    fn register_bit32(&mut self) {
        let u = Self::u32_ty;
        let f = Self::f32_ty;
        let v2 = Self::vec2_ty;
        let v4 = Self::vec4_ty;
        let v2u = Self::vec2u_ty;

        macro_rules! bit2 {
            ($($name:ident),*) => {
                $(self.add_ns("bit32", stringify!($name), vec![("a".into(), u()), ("b".into(), u())], u());)*
            };
        }
        bit2!(band, bor, bxor, lshift, rshift, arshift, lrotate, rrotate);

        self.add_ns("bit32", "bnot", vec![("x".into(), u())], u());
        self.add_ns("bit32", "extract", vec![("x".into(), u()), ("field".into(), u()), ("width".into(), u())], u());
        self.add_ns("bit32", "replace", vec![("x".into(), u()), ("v".into(), u()), ("field".into(), u()), ("width".into(), u())], u());
        self.add_ns("bit32", "countlz", vec![("x".into(), u())], u());
        self.add_ns("bit32", "counttz", vec![("x".into(), u())], u());
        self.add_ns("bit32", "countbits", vec![("x".into(), u())], u());
        self.add_ns("bit32", "reverse", vec![("x".into(), u())], u());
        self.add_ns("bit32", "byteswap", vec![("x".into(), u())], u());
        self.add_ns("bit32", "packf32", vec![("x".into(), f())], u());
        self.add_ns("bit32", "unpackf32", vec![("x".into(), u())], f());
        self.add_ns("bit32", "packUnorm4x8", vec![("v".into(), v4())], u());
        self.add_ns("bit32", "unpackUnorm4x8", vec![("p".into(), u())], v4());
        self.add_ns("bit32", "packSnorm4x8", vec![("v".into(), v4())], u());
        self.add_ns("bit32", "unpackSnorm4x8", vec![("p".into(), u())], v4());
        self.add_ns("bit32", "packHalf2x16", vec![("v".into(), v2())], u());
        self.add_ns("bit32", "unpackHalf2x16", vec![("p".into(), u())], v2());
        self.add_ns("bit32", "packUint2x16", vec![("v".into(), v2u())], u());
        self.add_ns("bit32", "unpackUint2x16", vec![("p".into(), u())], v2u());
    }

    fn register_utility(&mut self) {
        let f = Self::f32_ty;
        let b = Self::bool_ty;
        let bv3 = Self::bvec3_ty;

        self.add("select", vec![("cond".into(), b()), ("a".into(), f()), ("b".into(), f())], f());
        self.add("any", vec![("v".into(), bv3())], b());
        self.add("all", vec![("v".into(), bv3())], b());
        self.add("saturate", vec![("v".into(), f())], f());
        self.add("lerp", vec![("a".into(), f()), ("b".into(), f()), ("t".into(), f())], f());
        self.add("isinf", vec![("v".into(), f())], b());
        self.add("isnan", vec![("v".into(), f())], b());
        self.add("isfinite", vec![("v".into(), f())], b());

        self.add_with_stage("dFdx", vec![("v".into(), f())], f(), ShaderStage::Fragment);
        self.add_with_stage("dFdy", vec![("v".into(), f())], f(), ShaderStage::Fragment);
        self.add_with_stage("fwidth", vec![("v".into(), f())], f(), ShaderStage::Fragment);
        self.add_with_stage("discard", vec![], Type::Nil, ShaderStage::Fragment);

        // Workgroup barriers
        self.add_with_stage("workgroupBarrier", vec![], Type::Nil, ShaderStage::Compute);
        self.add_with_stage("memoryBarrier", vec![], Type::Nil, ShaderStage::Compute);
        self.add_with_stage("storageBarrier", vec![], Type::Nil, ShaderStage::Compute);
        self.add_with_stage("textureBarrier", vec![], Type::Nil, ShaderStage::Compute);

        // Subgroup operations
        macro_rules! subgroup {
            ($($name:ident),*) => {
                $(self.add_with_stage(stringify!($name), vec![("value".into(), f())], f(), ShaderStage::Compute);)*
            };
        }
        subgroup!(subgroupAdd, subgroupMin, subgroupMax, subgroupAnd, subgroupOr, subgroupXor);
        self.add_with_stage("subgroupBroadcast", vec![("value".into(), f()), ("id".into(), Self::u32_ty())], f(), ShaderStage::Compute);
        self.add_with_stage("subgroupShuffle", vec![("value".into(), f()), ("id".into(), Self::u32_ty())], f(), ShaderStage::Compute);
        self.add_with_stage("subgroupBallot", vec![("predicate".into(), b())], Self::vec4u_ty(), ShaderStage::Compute);
        self.add_with_stage("subgroupAll", vec![("predicate".into(), b())], b(), ShaderStage::Compute);
        self.add_with_stage("subgroupAny", vec![("predicate".into(), b())], b(), ShaderStage::Compute);
        self.add_with_stage("subgroupElect", vec![], b(), ShaderStage::Compute);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdlib_has_math() {
        let lib = StdLib::new();
        assert!(lib.lookup("sin").is_some());
        assert!(lib.lookup("cos").is_some());
        assert!(lib.lookup("sqrt").is_some());
        assert!(lib.lookup("lerp").is_some());
    }

    #[test]
    fn test_stdlib_has_vectors() {
        let lib = StdLib::new();
        assert!(lib.lookup("vector3.create").is_some());
        assert!(lib.lookup("vector2i.create").is_some());
        assert!(lib.lookup("bvector4.create").is_some());
    }

    #[test]
    fn test_stdlib_has_namespaces() {
        let lib = StdLib::new();
        assert!(lib.lookup_namespaced("noise", "perlin").is_some());
        assert!(lib.lookup_namespaced("bit32", "band").is_some());
        assert!(lib.lookup_namespaced("color", "ACESFilm").is_some());
        assert!(lib.lookup_namespaced("geo", "raySphere").is_some());
    }

    #[test]
    fn test_stdlib_has_texture() {
        let lib = StdLib::new();
        assert!(lib.lookup("sample").is_some());
        assert!(lib.lookup("sampleLod").is_some());
        assert!(lib.lookup("imageWrite").is_some());
        assert!(lib.lookup("imageRead").is_some());
    }

    #[test]
    fn test_stdlib_has_bitops() {
        let lib = StdLib::new();
        assert!(lib.lookup_namespaced("bit32", "band").is_some());
        assert!(lib.lookup_namespaced("bit32", "lshift").is_some());
        assert!(lib.lookup_namespaced("bit32", "countlz").is_some());
    }

    #[test]
    fn test_stdlib_has_derivatives() {
        let lib = StdLib::new();
        let dfdx = lib.lookup("dFdx").unwrap();
        assert_eq!(dfdx[0].min_stage, Some(ShaderStage::Fragment));
    }

    #[test]
    fn test_stdlib_has_barriers() {
        let lib = StdLib::new();
        let barrier = lib.lookup("workgroupBarrier").unwrap();
        assert_eq!(barrier[0].min_stage, Some(ShaderStage::Compute));
    }
}

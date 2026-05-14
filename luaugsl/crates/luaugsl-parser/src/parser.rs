use luaugsl_core::ast::*;
use luaugsl_core::error::{CompileError, SourcePos};
use luaugsl_core::types::*;
use luaugsl_lexer::token::Token;
use luaugsl_lexer::{Lexer, StringPart as LexerStringPart, TokenWithPos};

pub struct Parser {
    tokens: Vec<TokenWithPos>,
    pos: usize,
}

impl Parser {
    pub fn parse(input: &str) -> Result<Module, CompileError> {
        let lexer = Lexer::new(input);
        let tokens = lexer.tokenize()?;
        let mut parser = Parser { tokens, pos: 0 };
        parser.parse_module()
    }

    fn current(&self) -> &TokenWithPos {
        self.tokens.get(self.pos).unwrap_or_else(|| {
            self.tokens.last().unwrap_or_else(|| {
                panic!("Empty token stream")
            })
        })
    }

    fn peek_token(&self) -> &Token {
        &self.current().token
    }

    fn peek_pos(&self) -> SourcePos {
        self.current().pos
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].token.clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: Token) -> Result<SourcePos, CompileError> {
        let TokenWithPos { token, pos } = &self.tokens[self.pos];
        if token == &expected {
            let pos = *pos;
            self.advance();
            Ok(pos)
        } else {
            Err(CompileError::UnexpectedToken {
                expected: format!("{:?}", expected),
                found: format!("{:?}", token),
                pos: *pos,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<(String, SourcePos), CompileError> {
        match self.peek_token() {
            Token::Ident(name) => {
                let name = name.clone();
                let pos = self.peek_pos();
                self.advance();
                Ok((name, pos))
            }
            tok => Err(CompileError::UnexpectedToken {
                expected: "identifier".into(),
                found: format!("{:?}", tok),
                pos: self.peek_pos(),
            }),
        }
    }

    fn match_token(&mut self, expected: &Token) -> bool {
        if self.peek_token() == expected {
            self.advance();
            true
        } else {
            false
        }
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek_token(), Token::Eof)
    }

    // === Module ===

    fn parse_module(&mut self) -> Result<Module, CompileError> {
        let directives = Vec::new();
        let mut bindings = Vec::new();
        let mut type_aliases = Vec::new();
        let mut functions = Vec::new();
        let mut capabilities = Vec::new();
        let mut exports = Vec::new();
        let mut entry_point = None;

        // Parse module directives (--!strict, --!shader, etc.)
        // These are already handled as comments in the lexer, so we don't see them
        // In a real implementation, we'd extract directives before tokenizing

        // Parse declarations
        while !self.is_eof() {
            // Handle capability declarations first (they start with @)
            if matches!(self.peek_token(), Token::Attribute(name) if name == "capability") {
                let cap = self.parse_capability_decl()?;
                capabilities.push(cap);
                continue;
            }

            let attrs = self.parse_attributes();

            match self.peek_token() {
                Token::Export => {
                    self.advance();
                    match self.peek_token() {
                        Token::Type => {
                            let alias = self.parse_type_alias(true, attrs)?;
                            exports.push(alias.name.clone());
                            type_aliases.push(alias);
                        }
                        Token::Function => {
                            let func = self.parse_function(Some(attrs), true)?;
                            exports.push(func.name.clone());
                            if func.is_entry_point {
                                entry_point = Some(func.clone());
                            }
                            functions.push(func);
                        }
                        Token::Const => {
                            let binding = self.parse_binding_decl(Some(attrs), true)?;
                            exports.push(binding.name.clone());
                            bindings.push(binding);
                        }
                        _ => {
                            let func = self.parse_function(Some(attrs), true)?;
                            exports.push(func.name.clone());
                            if func.is_entry_point {
                                entry_point = Some(func.clone());
                            }
                            functions.push(func);
                        }
                    }
                }
                Token::Type => {
                    let alias = self.parse_type_alias(false, attrs)?;
                    type_aliases.push(alias);
                }
                Token::Const => {
                    let binding = self.parse_binding_decl(Some(attrs), false)?;
                    bindings.push(binding);
                }
                Token::Local => {
                    self.advance();
                    if self.match_token(&Token::Function) {
                        let func = self.parse_local_function()?;
                        functions.push(func);
                    } else {
                        self.pos -= 1;
                        let _stmt = self.parse_local_or_const_decl(false)?;
                    }
                }
                Token::Function => {
                    let func = self.parse_function(Some(attrs), false)?;
                    if func.is_entry_point {
                        entry_point = Some(func.clone());
                    }
                    functions.push(func);
                }
                Token::Eof => break,
                _ => {
                    if self.peek_token().is_stmt_start() {
                        let _stmt = self.parse_statement()?;
                    } else {
                        return Err(CompileError::UnexpectedToken {
                            expected: "declaration".into(),
                            found: format!("{:?}", self.peek_token()),
                            pos: self.peek_pos(),
                        });
                    }
                }
            }
        }

        Ok(Module {
            directives,
            bindings,
            type_aliases,
            functions,
            capabilities,
            exports,
            entry_point,
        })
    }

    // === Attributes ===

    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        while let Token::Attribute(name) = self.peek_token() {
            let name = name.clone();
            self.advance();
            let args = if self.match_token(&Token::LParen) {
                let args = self.parse_attr_args();
                let _ = self.expect(Token::RParen);
                args
            } else {
                Vec::new()
            };
            attrs.push(Attribute { name, args });
        }
        attrs
    }

    fn parse_attr_args(&mut self) -> Vec<AttrArg> {
        let mut args = Vec::new();
        loop {
            match self.peek_token() {
                Token::String(s) => {
                    args.push(AttrArg::String(s.clone()));
                    self.advance();
                }
                Token::Number(n) => {
                    args.push(AttrArg::Number(*n));
                    self.advance();
                }
                Token::Ident(s) => {
                    args.push(AttrArg::Ident(s.clone()));
                    self.advance();
                }
                _ => break,
            }
            if !self.match_token(&Token::Comma) {
                break;
            }
        }
        args
    }

    // === Declarations ===

    fn parse_type_alias(&mut self, is_export: bool, _attrs: Vec<Attribute>) -> Result<TypeAlias, CompileError> {
        self.expect(Token::Type)?;
        let (name, _pos) = self.expect_ident()?;
        let params = if self.match_token(&Token::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };
        self.expect(Token::Eq)?;
        let ty = self.parse_type()?;
        // Optional semicolon
        self.match_token(&Token::Semicolon);

        Ok(TypeAlias { is_export, name, params, ty })
    }

    fn parse_type_params(&mut self) -> Result<Vec<String>, CompileError> {
        let mut params = Vec::new();
        loop {
            let (name, _) = self.expect_ident()?;
            params.push(name);
            if !self.match_token(&Token::Comma) {
                break;
            }
        }
        self.expect(Token::Gt)?;
        Ok(params)
    }

    fn parse_binding_decl(&mut self, attrs: Option<Vec<Attribute>>, is_export: bool) -> Result<BindingDecl, CompileError> {
        let attributes = attrs.unwrap_or_default();
        self.expect(Token::Const)?;
        let (name, _) = self.expect_ident()?;
        self.expect(Token::Eq)?;

        let (value, ty) = self.parse_binding_value()?;

        Ok(BindingDecl {
            attributes,
            is_const: true,
            is_export,
            name,
            value,
            ty,
        })
    }

    fn parse_binding_value(&mut self) -> Result<(BindingValue, Type), CompileError> {
        match self.peek_token() {
            Token::Uniform => {
                self.advance();
                // Expect a table type definition: { field: type, ... }
                let fields = self.parse_table_fields_for_type()?;
                let ty = Type::UniformBuffer(fields.clone());
                Ok((BindingValue::Uniform { fields }, ty))
            }
            Token::Texture2D => {
                self.advance();
                Ok((BindingValue::Texture { kind: TextureKind::Texture2D }, Type::Texture2D))
            }
            Token::Texture3D => {
                self.advance();
                Ok((BindingValue::Texture { kind: TextureKind::Texture3D }, Type::Texture3D))
            }
            Token::TextureCube => {
                self.advance();
                Ok((BindingValue::Texture { kind: TextureKind::TextureCube }, Type::TextureCube))
            }
            Token::Texture2DArray => {
                self.advance();
                Ok((BindingValue::Texture { kind: TextureKind::Texture2DArray }, Type::Texture2DArray))
            }
            Token::Sampler => {
                self.advance();
                Ok((BindingValue::Sampler, Type::Sampler))
            }
            Token::StorageBuffer => {
                self.advance();
                self.expect(Token::Lt)?;
                let elem_ty = self.parse_type()?;
                let access = if self.match_token(&Token::Comma) {
                    // parse access modifier
                    Access::ReadWrite
                } else {
                    Access::ReadWrite
                };
                self.expect(Token::Gt)?;
                let ty = Type::StorageBuffer { elem_ty: Box::new(elem_ty.clone()), access: access.clone() };
                Ok((BindingValue::StorageBuffer { elem_ty, access }, ty))
            }
            Token::StorageImage => {
                self.advance();
                self.expect(Token::Lt)?;
                let format = match self.peek_token() {
                    Token::Ident(s) | Token::String(s) => {
                        let s = s.clone();
                        self.advance();
                        s
                    }
                    _ => "rgba8".into(),
                };
                self.expect(Token::Gt)?;
                Ok((BindingValue::StorageImage { format: format.clone() }, Type::StorageImage { format }))
            }
            _ => {
                let expr = self.parse_expression()?;
                let ty = Type::Unknown; // Will be inferred
                Ok((BindingValue::Expr(expr), ty))
            }
        }
    }

    fn parse_capability_decl(&mut self) -> Result<CapabilityDecl, CompileError> {
        self.expect(Token::Attribute("capability".into()))?;
        self.expect(Token::LParen)?;
        let (name, _) = self.expect_ident()?;
        self.expect(Token::RParen)?;

        let cap = match name.as_str() {
            "textureSample" => Capability::TextureSample,
            "storageRead" => Capability::StorageRead,
            "storageWrite" => Capability::StorageWrite,
            "atomicOps" => Capability::AtomicOps,
            "derivatives" => Capability::Derivatives,
            "subgroupOps" => Capability::SubgroupOps,
            "rayTracing" => Capability::RayTracing,
            "meshShader" => Capability::MeshShader,
            "discard" => Capability::Discard,
            "imageReadWrite" => Capability::ImageReadWrite,
            "pushConstant" => Capability::PushConstant,
            _ => return Err(CompileError::General(format!("unknown capability: {}", name))),
        };

        Ok(CapabilityDecl { cap })
    }

    fn parse_function(&mut self, attrs: Option<Vec<Attribute>>, is_export: bool) -> Result<FunctionDecl, CompileError> {
        let attributes = attrs.unwrap_or_default();
        let is_entry_point = attributes.iter().any(|a| a.name == "stage");
        let stage = attributes.iter().find(|a| a.name == "stage").and_then(|a| {
            a.args.first().and_then(|arg| {
                if let AttrArg::String(s) = arg {
                    s.parse().ok()
                } else if let AttrArg::Ident(s) = arg {
                    s.parse().ok()
                } else {
                    None
                }
            })
        });

        self.expect(Token::Function)?;
        let (name, _) = self.expect_ident()?;

        // Generic parameters
        let generic_params = if self.match_token(&Token::Lt) {
            self.parse_type_params()?
        } else {
            Vec::new()
        };

        self.expect(Token::LParen)?;
        let params = self.parse_param_list()?;

        self.expect(Token::Colon)?;
        let return_type = self.parse_type()?;

        let body = self.parse_block()?;
        self.expect(Token::End)?;

        Ok(FunctionDecl {
            attributes,
            is_export,
            name,
            generic_params,
            params,
            return_type,
            body,
            is_entry_point,
            stage,
        })
    }

    fn parse_local_function(&mut self) -> Result<FunctionDecl, CompileError> {
        // Already consumed "local function"
        let (name, _) = self.expect_ident()?;

        self.expect(Token::LParen)?;
        let params = self.parse_param_list()?;

        self.expect(Token::Colon)?;
        let return_type = self.parse_type()?;

        let body = self.parse_block()?;
        self.expect(Token::End)?;

        Ok(FunctionDecl {
            attributes: Vec::new(),
            is_export: false,
            name,
            generic_params: Vec::new(),
            params,
            return_type,
            body,
            is_entry_point: false,
            stage: None,
        })
    }

    fn parse_param_list(&mut self) -> Result<Vec<Param>, CompileError> {
        let mut params = Vec::new();

        // If next is ), empty list
        if self.match_token(&Token::RParen) {
            return Ok(params);
        }

        loop {
            let attrs = self.parse_attributes();
            let (name, _) = self.expect_ident()?;
            let ty = if self.match_token(&Token::Colon) {
                self.parse_type()?
            } else {
                Type::Unknown
            };

            let builtin = attrs.iter().find(|a| a.name == "builtin").and_then(|a| {
                a.args.first().map(|arg| match arg {
                    AttrArg::String(s) | AttrArg::Ident(s) => s.clone(),
                    _ => String::new(),
                })
            });
            let interpolate = attrs.iter().find(|a| a.name == "interpolate").and_then(|a| {
                a.args.first().map(|arg| match arg {
                    AttrArg::String(s) | AttrArg::Ident(s) => s.clone(),
                    _ => String::new(),
                })
            });
            let location = attrs.iter().find(|a| a.name == "location").and_then(|a| {
                a.args.first().and_then(|arg| match arg {
                    AttrArg::Number(n) => Some(*n as u32),
                    _ => None,
                })
            });

            params.push(Param {
                name,
                ty,
                builtin,
                interpolate,
                location,
            });

            if !self.match_token(&Token::Comma) {
                break;
            }
        }

        self.expect(Token::RParen)?;
        Ok(params)
    }

    // === Types ===

    pub fn parse_type(&mut self) -> Result<Type, CompileError> {
        self.parse_type_union()
    }

    fn parse_type_union(&mut self) -> Result<Type, CompileError> {
        let mut types = vec![self.parse_type_intersection()?];
        while self.match_token(&Token::Pipe) {
            types.push(self.parse_type_intersection()?);
        }
        if types.len() == 1 {
            Ok(types.into_iter().next().unwrap())
        } else {
            Ok(Type::Union(types))
        }
    }

    fn parse_type_intersection(&mut self) -> Result<Type, CompileError> {
        let mut types = vec![self.parse_type_postfix()?];
        while self.match_token(&Token::Ampersand) {
            types.push(self.parse_type_postfix()?);
        }
        if types.len() == 1 {
            Ok(types.into_iter().next().unwrap())
        } else {
            Ok(Type::Intersection(types))
        }
    }

    fn parse_type_postfix(&mut self) -> Result<Type, CompileError> {
        let mut ty = self.parse_type_primary()?;
        // Optional type: T?
        if self.match_token(&Token::Or) {
            // Actually, we need to check if next is `nil` for optional
            // This is tricky. Let me handle `?` as a separate token concept.
        }
        // Array type: {T, N}
        if self.match_token(&Token::LBrace) {
            // Could be array type or table type
            if matches!(self.peek_token(), Token::RBrace) {
                // Empty braces — probably a typo, but accept
                self.expect(Token::RBrace)?;
            } else {
                // Check if this is an array type {T} or {T, N}
                let inner = self.parse_type()?;
                if self.match_token(&Token::Comma) {
                    // Array with size
                    if let Token::Number(n) = self.peek_token() {
                        let size = *n as u32;
                        self.advance();
                        self.expect(Token::RBrace)?;
                        ty = Type::Array(Box::new(inner), Some(size));
                    } else {
                        // Table type
                        self.pos -= 1; // put back comma
                        let fields = self.parse_table_type_fields(inner)?;
                        ty = Type::Table(fields, None);
                    }
                } else {
                    self.expect(Token::RBrace)?;
                    ty = Type::Array(Box::new(inner), None);
                }
            }
        }
        Ok(ty)
    }

    fn parse_type_primary(&mut self) -> Result<Type, CompileError> {
        match self.peek_token() {
            Token::Nil => { self.advance(); Ok(Type::Nil) }
            Token::True | Token::False => { self.advance(); Ok(Type::Bool) }
            Token::F32 => { self.advance(); Ok(Type::F32) }
            Token::I32 => { self.advance(); Ok(Type::I32) }
            Token::U32 => { self.advance(); Ok(Type::U32) }
            Token::Bool => { self.advance(); Ok(Type::Bool) }
            Token::Number(_) => { self.advance(); Ok(Type::F32) }
            Token::Vector2 => { self.advance(); Ok(Type::Vec2) }
            Token::Vector3 => { self.advance(); Ok(Type::Vec3) }
            Token::Vector4 => { self.advance(); Ok(Type::Vec4) }
            Token::Vector2i => { self.advance(); Ok(Type::Vec2i) }
            Token::Vector3i => { self.advance(); Ok(Type::Vec3i) }
            Token::Vector4i => { self.advance(); Ok(Type::Vec4i) }
            Token::Vector2u => { self.advance(); Ok(Type::Vec2u) }
            Token::Vector3u => { self.advance(); Ok(Type::Vec3u) }
            Token::Vector4u => { self.advance(); Ok(Type::Vec4u) }
            Token::BVector2 => { self.advance(); Ok(Type::BVec2) }
            Token::BVector3 => { self.advance(); Ok(Type::BVec3) }
            Token::BVector4 => { self.advance(); Ok(Type::BVec4) }
            Token::Mat2x2 => { self.advance(); Ok(Type::Mat2x2) }
            Token::Mat3x3 => { self.advance(); Ok(Type::Mat3x3) }
            Token::Mat4x4 => { self.advance(); Ok(Type::Mat4x4) }
            Token::Texture2D => { self.advance(); Ok(Type::Texture2D) }
            Token::Texture3D => { self.advance(); Ok(Type::Texture3D) }
            Token::TextureCube => { self.advance(); Ok(Type::TextureCube) }
            Token::Texture2DArray => { self.advance(); Ok(Type::Texture2DArray) }
            Token::TextureCubeArray => { self.advance(); Ok(Type::TextureCubeArray) }
            Token::StorageBuffer => { self.advance(); Ok(Type::StorageBuffer { elem_ty: Box::new(Type::Unknown), access: Access::ReadWrite }) }
            Token::StorageImage => { self.advance(); Ok(Type::StorageImage { format: "rgba8".into() }) }
            Token::Sampler => { self.advance(); Ok(Type::Sampler) }
            Token::Uniform => { self.advance(); Ok(Type::UniformBuffer(Vec::new())) }
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();
                if self.match_token(&Token::Lt) {
                    // Generic instantiation: Type<Args>
                    let mut args = Vec::new();
                    loop {
                        args.push(self.parse_type()?);
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(Token::Gt)?;
                    Ok(Type::Generic(name, args))
                } else {
                    Ok(Type::Generic(name, Vec::new()))
                }
            }
            Token::LParen => {
                self.advance();
                let ty = self.parse_type()?;
                self.expect(Token::RParen)?;
                Ok(ty)
            }
            Token::LBrace => {
                self.advance();
                // Table type { field: type, ... }
                let fields = self.parse_table_type_rest()?;
                Ok(Type::Table(fields, None))
            }
            Token::Function => {
                self.advance();
                self.expect(Token::LParen)?;
                let mut params = Vec::new();
                if !matches!(self.peek_token(), Token::RParen) {
                    loop {
                        params.push(self.parse_type()?);
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                }
                self.expect(Token::RParen)?;
                self.expect(Token::Arrow)?;
                let ret = self.parse_type()?;
                Ok(Type::Function(params, Box::new(ret)))
            }
            _ => Err(CompileError::UnexpectedToken {
                expected: "type".into(),
                found: format!("{:?}", self.peek_token()),
                pos: self.peek_pos(),
            }),
        }
    }

    fn parse_table_type_rest(&mut self) -> Result<Vec<FieldDef>, CompileError> {
        let mut fields = Vec::new();
        while !matches!(self.peek_token(), Token::RBrace | Token::Eof) {
            let field = self.parse_field_def()?;
            fields.push(field);
            if !self.match_token(&Token::Comma) {
                break;
            }
        }
        self.expect(Token::RBrace)?;
        Ok(fields)
    }

    fn parse_table_type_fields(&mut self, first_field_type: Type) -> Result<Vec<FieldDef>, CompileError> {
        // This is for when we've already parsed the first type in {T, ...}
        // and realized it's a table type, not an array type
        let mut fields = Vec::new();
        // We need to re-parse. The type we consumed was actually a field name's type annotation
        // This is a simplified version
        fields.push(FieldDef {
            name: String::new(),
            ty: first_field_type,
            access: None,
            offset: None,
            align: None,
            location: None,
            interpolate: None,
            builtin: None,
        });
        while !matches!(self.peek_token(), Token::RBrace | Token::Eof) {
            let field = self.parse_field_def()?;
            fields.push(field);
            if !self.match_token(&Token::Comma) {
                break;
            }
        }
        self.expect(Token::RBrace)?;
        Ok(fields)
    }

    fn parse_field_def(&mut self) -> Result<FieldDef, CompileError> {
        // Parse attributes for field
        let attrs = self.parse_attributes();
        let (name, _) = self.expect_ident()?;
        self.expect(Token::Colon)?;
        let ty = self.parse_type()?;

        let location = attrs.iter().find(|a| a.name == "location").and_then(|a| {
            a.args.first().and_then(|arg| match arg {
                AttrArg::Number(n) => Some(*n as u32),
                _ => None,
            })
        });
        let builtin = attrs.iter().find(|a| a.name == "builtin").and_then(|a| {
            a.args.first().map(|arg| match arg {
                AttrArg::String(s) | AttrArg::Ident(s) => s.clone(),
                _ => String::new(),
            })
        });
        let interpolate = attrs.iter().find(|a| a.name == "interpolate").and_then(|a| {
            a.args.first().map(|arg| match arg {
                AttrArg::String(s) | AttrArg::Ident(s) => s.clone(),
                _ => String::new(),
            })
        });

        Ok(FieldDef {
            name,
            ty,
            access: None,
            offset: None,
            align: None,
            location,
            interpolate,
            builtin,
        })
    }

    fn parse_table_fields_for_type(&mut self) -> Result<Vec<FieldDef>, CompileError> {
        self.expect(Token::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek_token(), Token::RBrace | Token::Eof) {
            let field = self.parse_field_def()?;
            fields.push(field);
            if !self.match_token(&Token::Comma) {
                break;
            }
        }
        self.expect(Token::RBrace)?;
        Ok(fields)
    }

    // === Statements ===

    fn parse_block(&mut self) -> Result<Block, CompileError> {
        let mut stmts = Vec::new();
        let mut ret = None;

        while !self.is_block_end() {
            if matches!(self.peek_token(), Token::Return) {
                ret = Some(self.parse_return_stmt()?);
                break;
            }
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
        }

        Ok(Block { stmts, ret })
    }

    fn is_block_end(&self) -> bool {
        matches!(self.peek_token(), Token::End | Token::Else | Token::Elseif | Token::Until | Token::Eof)
    }

    fn parse_statement(&mut self) -> Result<Stmt, CompileError> {
        match self.peek_token() {
            Token::Semicolon => { self.advance(); Ok(Stmt::Empty) }
            Token::Local => self.parse_local_or_const_decl(false),
            Token::Const => self.parse_local_or_const_decl(true),
            Token::If => self.parse_if_stmt(),
            Token::For => self.parse_for_stmt(),
            Token::While => self.parse_while_stmt(),
            Token::Repeat => self.parse_repeat_stmt(),
            Token::Do => { self.advance(); let block = self.parse_block()?; self.expect(Token::End)?; Ok(Stmt::DoBlock(block)) }
            Token::Return => { let ret = self.parse_return_stmt()?; Ok(Stmt::Return(ret)) }
            Token::Break => { self.advance(); Ok(Stmt::Break) }
            Token::Continue => { self.advance(); Ok(Stmt::Continue) }
            Token::Type => {
                let alias = self.parse_type_alias(false, Vec::new())?;
                Ok(Stmt::TypeAlias(alias))
            }
            _ => {
                // Could be assignment or function call
                let expr = self.parse_expression()?;
                match self.peek_token() {
                    Token::Eq | Token::PlusEq | Token::MinusEq | Token::StarEq
                    | Token::SlashEq | Token::FloorDivEq | Token::PercentEq
                    | Token::CaretEq | Token::ConcatEq => {
                        self.parse_assignment(expr)
                    }
                    _ => Ok(Stmt::Call(expr)),
                }
            }
        }
    }

    fn parse_local_or_const_decl(&mut self, is_const: bool) -> Result<Stmt, CompileError> {
        self.advance(); // local or const
        let mut names = Vec::new();
        loop {
            let (name, _) = self.expect_ident()?;
            let ty = if self.match_token(&Token::Colon) {
                Some(self.parse_type()?)
            } else {
                None
            };
            names.push((name, ty));
            if !self.match_token(&Token::Comma) {
                break;
            }
        }

        let values = if self.match_token(&Token::Eq) {
            self.parse_expr_list()?
        } else {
            Vec::new()
        };

        if is_const {
            Ok(Stmt::ConstDecl { names, values, attributes: Vec::new() })
        } else {
            Ok(Stmt::LocalDecl { names, values, attributes: Vec::new() })
        }
    }

    fn parse_assignment(&mut self, target_expr: Expr) -> Result<Stmt, CompileError> {
        let target = self.expr_to_assign_target(target_expr)?;
        let op = match self.advance() {
            Token::Eq => None,
            Token::PlusEq => Some(BinOp::Add),
            Token::MinusEq => Some(BinOp::Sub),
            Token::StarEq => Some(BinOp::Mul),
            Token::SlashEq => Some(BinOp::Div),
            Token::FloorDivEq => Some(BinOp::FloorDiv),
            Token::PercentEq => Some(BinOp::Mod),
            Token::CaretEq => Some(BinOp::Pow),
            Token::ConcatEq => Some(BinOp::Concat),
            _ => return Err(CompileError::InvalidAssignmentTarget(self.peek_pos())),
        };

        let value = self.parse_expression()?;

        match op {
            Some(op) => Ok(Stmt::CompoundAssign { target, op, value }),
            None => Ok(Stmt::Assign { target, value }),
        }
    }

    fn expr_to_assign_target(&self, expr: Expr) -> Result<AssignTarget, CompileError> {
        match expr {
            Expr::Identifier(name) => Ok(AssignTarget::Var(name)),
            Expr::Member { base, member } => Ok(AssignTarget::Field { base, field: member }),
            Expr::Index { base, index } => Ok(AssignTarget::Index { base, index }),
            Expr::Swizzle { base, components } => Ok(AssignTarget::Swizzle { base, components }),
            _ => Err(CompileError::InvalidAssignmentTarget(self.peek_pos())),
        }
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, CompileError> {
        self.expect(Token::If)?;
        let cond = self.parse_expression()?;
        self.expect(Token::Then)?;
        let then_block = self.parse_block()?;

        let mut elseifs = Vec::new();
        while self.match_token(&Token::Elseif) {
            let cond = self.parse_expression()?;
            self.expect(Token::Then)?;
            let block = self.parse_block()?;
            elseifs.push((cond, block));
        }

        let else_block = if self.match_token(&Token::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };

        self.expect(Token::End)?;

        Ok(Stmt::If { cond, then_block, elseifs, else_block })
    }

    fn parse_for_stmt(&mut self) -> Result<Stmt, CompileError> {
        self.expect(Token::For)?;
        let (name, _) = self.expect_ident()?;

        // Numeric for: for i = start, end, step do
        if self.match_token(&Token::Eq) {
            let start = self.parse_expression()?;
            self.expect(Token::Comma)?;
            let end = self.parse_expression()?;
            let step = if self.match_token(&Token::Comma) {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.expect(Token::Do)?;
            let body = self.parse_block()?;
            self.expect(Token::End)?;
            Ok(Stmt::ForNumeric { var: name, start, end, step, body })
        } else {
            // Generalized for: for a, b in expr do
            let mut vars = vec![name];
            while self.match_token(&Token::Comma) {
                let (name, _) = self.expect_ident()?;
                vars.push(name);
            }
            self.expect(Token::In)?;
            let expr = self.parse_expression()?;
            self.expect(Token::Do)?;
            let body = self.parse_block()?;
            self.expect(Token::End)?;
            Ok(Stmt::ForGeneral { vars, expr, body })
        }
    }

    fn parse_while_stmt(&mut self) -> Result<Stmt, CompileError> {
        self.expect(Token::While)?;
        let cond = self.parse_expression()?;
        self.expect(Token::Do)?;
        let body = self.parse_block()?;
        self.expect(Token::End)?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_repeat_stmt(&mut self) -> Result<Stmt, CompileError> {
        self.expect(Token::Repeat)?;
        let body = self.parse_block()?;
        self.expect(Token::Until)?;
        let cond = self.parse_expression()?;
        Ok(Stmt::Repeat { body, cond })
    }

    fn parse_return_stmt(&mut self) -> Result<Vec<Expr>, CompileError> {
        self.expect(Token::Return)?;
        if self.is_block_end() {
            Ok(Vec::new())
        } else {
            let exprs = self.parse_expr_list()?;
            Ok(exprs)
        }
    }

    // === Expressions ===

    fn parse_expression(&mut self) -> Result<Expr, CompileError> {
        self.parse_or_expr()
    }

    fn parse_expr_list(&mut self) -> Result<Vec<Expr>, CompileError> {
        let mut exprs = vec![self.parse_expression()?];
        while self.match_token(&Token::Comma) {
            exprs.push(self.parse_expression()?);
        }
        Ok(exprs)
    }

    fn parse_or_expr(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_and_expr()?;
        while self.match_token(&Token::Or) {
            let right = self.parse_and_expr()?;
            left = Expr::Binary { op: BinOp::Or, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_cmp_expr()?;
        while self.match_token(&Token::And) {
            let right = self.parse_cmp_expr()?;
            left = Expr::Binary { op: BinOp::And, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_cmp_expr(&mut self) -> Result<Expr, CompileError> {
        let left = self.parse_concat_expr()?;
        let op = match self.peek_token() {
            Token::EqEq => { self.advance(); BinOp::Eq }
            Token::TildeEq => { self.advance(); BinOp::NotEq }
            Token::Lt => { self.advance(); BinOp::Lt }
            Token::Le => { self.advance(); BinOp::Le }
            Token::Gt => { self.advance(); BinOp::Gt }
            Token::Ge => { self.advance(); BinOp::Ge }
            _ => return Ok(left),
        };
        let right = self.parse_concat_expr()?;
        Ok(Expr::Binary { op, left: Box::new(left), right: Box::new(right) })
    }

    fn parse_concat_expr(&mut self) -> Result<Expr, CompileError> {
        let left = self.parse_add_expr()?;
        if self.match_token(&Token::Concat) {
            let right = self.parse_concat_expr()?;
            Ok(Expr::Binary { op: BinOp::Concat, left: Box::new(left), right: Box::new(right) })
        } else {
            Ok(left)
        }
    }

    fn parse_add_expr(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_mul_expr()?;
        loop {
            let op = match self.peek_token() {
                Token::Plus => { self.advance(); BinOp::Add }
                Token::Minus => { self.advance(); BinOp::Sub }
                _ => break,
            };
            let right = self.parse_mul_expr()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_mul_expr(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_pow_expr()?;
        loop {
            let op = match self.peek_token() {
                Token::Star => { self.advance(); BinOp::Mul }
                Token::Slash => { self.advance(); BinOp::Div }
                Token::FloorDiv => { self.advance(); BinOp::FloorDiv }
                Token::Percent => { self.advance(); BinOp::Mod }
                _ => break,
            };
            let right = self.parse_pow_expr()?;
            left = Expr::Binary { op, left: Box::new(left), right: Box::new(right) };
        }
        Ok(left)
    }

    fn parse_pow_expr(&mut self) -> Result<Expr, CompileError> {
        let left = self.parse_unary_expr()?;
        if self.match_token(&Token::Caret) {
            let right = self.parse_pow_expr()?;
            Ok(Expr::Binary { op: BinOp::Pow, left: Box::new(left), right: Box::new(right) })
        } else {
            Ok(left)
        }
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, CompileError> {
        let op = match self.peek_token() {
            Token::Not => { self.advance(); Some(UnOp::Not) }
            Token::Minus => { self.advance(); Some(UnOp::Neg) }
            Token::Hash => { self.advance(); Some(UnOp::Len) }
            Token::Tilde => { self.advance(); Some(UnOp::BitNot) }
            _ => None,
        };

        if let Some(op) = op {
            let operand = Box::new(self.parse_unary_expr()?);
            Ok(Expr::Unary { op, operand })
        } else {
            self.parse_cast_expr()
        }
    }

    fn parse_cast_expr(&mut self) -> Result<Expr, CompileError> {
        let expr = self.parse_postfix_expr()?;
        if self.match_token(&Token::DoubleColon) {
            let ty = self.parse_type()?;
            Ok(Expr::Cast { expr: Box::new(expr), ty })
        } else {
            Ok(expr)
        }
    }

    fn parse_postfix_expr(&mut self) -> Result<Expr, CompileError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            match self.peek_token() {
                Token::LParen => {
                    self.advance();
                    let args = if matches!(self.peek_token(), Token::RParen) {
                        Vec::new()
                    } else {
                        self.parse_expr_list()?
                    };
                    self.expect(Token::RParen)?;
                    expr = Expr::Call { func: Box::new(expr), args };
                }
                Token::LBracket => {
                    self.advance();
                    let index = Box::new(self.parse_expression()?);
                    self.expect(Token::RBracket)?;
                    expr = Expr::Index { base: Box::new(expr), index };
                }
                Token::Dot => {
                    self.advance();
                    let (member, _) = self.expect_ident()?;
                    // Check if this is a method call
                    if matches!(self.peek_token(), Token::LParen) {
                        self.advance(); // (
                        let args = if matches!(self.peek_token(), Token::RParen) {
                            Vec::new()
                        } else {
                            self.parse_expr_list()?
                        };
                        self.expect(Token::RParen)?;
                        expr = Expr::MethodCall { base: Box::new(expr), method: member, args };
                    } else {
                        expr = Expr::Member { base: Box::new(expr), member };
                    }
                }
                Token::Colon => {
                    self.advance();
                    let (method, _) = self.expect_ident()?;
                    self.expect(Token::LParen)?;
                    let args = if matches!(self.peek_token(), Token::RParen) {
                        Vec::new()
                    } else {
                        self.parse_expr_list()?
                    };
                    self.expect(Token::RParen)?;
                    expr = Expr::MethodCall { base: Box::new(expr), method, args };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, CompileError> {
        match self.peek_token() {
            Token::Nil => { self.advance(); Ok(Expr::Literal(Literal::Nil)) }
            Token::True => { self.advance(); Ok(Expr::Literal(Literal::Bool(true))) }
            Token::False => { self.advance(); Ok(Expr::Literal(Literal::Bool(false))) }
            Token::Number(n) => {
                let n = *n;
                self.advance();
                Ok(Expr::Literal(Literal::Number(n)))
            }
            Token::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Literal(Literal::String(s)))
            }
            Token::Interpolated(parts) => {
                let parts = parts.clone();
                self.advance();
                let ast_parts = parts.into_iter().map(|p| match p {
                    LexerStringPart::Literal(s) => StringPart::Literal(s),
                    LexerStringPart::Expr(s) => StringPart::Expr(Expr::Identifier(s)),
                }).collect();
                Ok(Expr::InterpolatedString(ast_parts))
            }
            Token::Ident(_)
            | Token::Vector2 | Token::Vector3 | Token::Vector4
            | Token::Vector2i | Token::Vector3i | Token::Vector4i
            | Token::Vector2u | Token::Vector3u | Token::Vector4u
            | Token::BVector2 | Token::BVector3 | Token::BVector4
            | Token::Mat2x2 | Token::Mat3x3 | Token::Mat4x4
            | Token::F32 | Token::I32 | Token::U32 | Token::Bool
            | Token::Texture2D | Token::Texture3D | Token::TextureCube
            | Token::Texture2DArray | Token::TextureCubeArray
            | Token::Uniform | Token::Sampler | Token::StorageBuffer | Token::StorageImage => {
                let name = match self.peek_token() {
                    Token::Ident(name) => name.clone(),
                    Token::Vector2 => "vector2".to_string(),
                    Token::Vector3 => "vector3".to_string(),
                    Token::Vector4 => "vector4".to_string(),
                    Token::Vector2i => "vector2i".to_string(),
                    Token::Vector3i => "vector3i".to_string(),
                    Token::Vector4i => "vector4i".to_string(),
                    Token::Vector2u => "vector2u".to_string(),
                    Token::Vector3u => "vector3u".to_string(),
                    Token::Vector4u => "vector4u".to_string(),
                    Token::BVector2 => "bvector2".to_string(),
                    Token::BVector3 => "bvector3".to_string(),
                    Token::BVector4 => "bvector4".to_string(),
                    Token::Mat2x2 => "mat2x2".to_string(),
                    Token::Mat3x3 => "mat3x3".to_string(),
                    Token::Mat4x4 => "mat4x4".to_string(),
                    Token::F32 => "f32".to_string(),
                    Token::I32 => "i32".to_string(),
                    Token::U32 => "u32".to_string(),
                    Token::Bool => "bool".to_string(),
                    Token::Texture2D => "texture2d".to_string(),
                    Token::Texture3D => "texture3d".to_string(),
                    Token::TextureCube => "texturecube".to_string(),
                    Token::Texture2DArray => "texture2darray".to_string(),
                    Token::TextureCubeArray => "texturecubearray".to_string(),
                    Token::Uniform => "uniform".to_string(),
                    Token::Sampler => "sampler".to_string(),
                    Token::StorageBuffer => "storagebuffer".to_string(),
                    Token::StorageImage => "storageimage".to_string(),
                    _ => unreachable!(),
                };
                self.advance();

                // Check for namespace.function pattern: noise.perlin, bit32.band, etc.
                if self.match_token(&Token::Dot) {
                    if let Token::Ident(method) | Token::String(method) = self.peek_token() {
                        let method = method.clone();
                        self.advance();
                        let full_name = format!("{}.{}", name, method);

                        // Check if it's a constructor call
                        if matches!(self.peek_token(), Token::LParen) {
                            return Ok(Expr::Identifier(full_name));
                        }

                        return Ok(Expr::Member {
                            base: Box::new(Expr::Identifier(name)),
                            member: method,
                        });
                    }
                }

                // Vector constructors: vectorN.create(...)
                if name.starts_with("vector") || name.starts_with("bvector") || name.starts_with("mat") {
                    if matches!(self.peek_token(), Token::LParen) {
                        // Will be handled by parse_postfix_expr as a call
                    }
                }

                Ok(Expr::Identifier(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(Token::RParen)?;
                Ok(Expr::Grouped(Box::new(expr)))
            }
            Token::LBrace => {
                self.parse_table_constructor()
            }
            Token::Function => {
                // Inline function expressions are not supported in LuauGSL
                return Err(CompileError::UnexpectedToken {
                    expected: "expression".into(),
                    found: "function keyword".into(),
                    pos: self.peek_pos(),
                });
            }
            _ => Err(CompileError::UnexpectedToken {
                expected: "expression".into(),
                found: format!("{:?}", self.peek_token()),
                pos: self.peek_pos(),
            }),
        }
    }

    fn parse_table_constructor(&mut self) -> Result<Expr, CompileError> {
        self.expect(Token::LBrace)?;
        let mut fields = Vec::new();

        while !matches!(self.peek_token(), Token::RBrace | Token::Eof) {
            // Named field: name = expr
            if matches!(self.peek_token(), Token::Ident(_)) {
                // Look ahead to see if it's `name =` or just an expression
                let saved_pos = self.pos;
                let (name, _) = self.expect_ident()?;
                if self.match_token(&Token::Eq) {
                    let expr = self.parse_expression()?;
                    fields.push((Some(name), expr));
                } else {
                    // It's an array element, not a named field
                    self.pos = saved_pos;
                    let expr = self.parse_expression()?;
                    fields.push((None, expr));
                }
            } else {
                let expr = self.parse_expression()?;
                fields.push((None, expr));
            }

            if !self.match_token(&Token::Comma) && !self.match_token(&Token::Semicolon) {
                break;
            }
        }

        self.expect(Token::RBrace)?;

        // If all fields are unnamed, it's an array; otherwise it's a table
        let all_unnamed = fields.iter().all(|(name, _)| name.is_none());
        if all_unnamed && !fields.is_empty() {
            Ok(Expr::Array(fields.into_iter().map(|(_, v)| v).collect()))
        } else {
            Ok(Expr::Table(fields))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_fragment() {
        let src = r#"
            @stage("fragment")
            export function main(input: f32): f32
                return input
            end
        "#;
        let module = Parser::parse(src).unwrap();
        assert!(module.entry_point.is_some());
        assert_eq!(module.entry_point.as_ref().unwrap().name, "main");
        assert_eq!(module.entry_point.as_ref().unwrap().stage, Some(ShaderStage::Fragment));
    }

    #[test]
    fn test_parse_bindings() {
        let src = r#"
            @binding(0, 0)
            const LightData = uniform { direction: vector3, intensity: f32 }
            @binding(0, 1)
            const AlbedoTexture = texture2d
            @stage("fragment")
            export function main(input: f32): f32
                return input
            end
        "#;
        let module = Parser::parse(src).unwrap();
        assert_eq!(module.bindings.len(), 2);
        assert_eq!(module.bindings[0].name, "LightData");
        assert_eq!(module.bindings[1].name, "AlbedoTexture");
    }

    #[test]
    fn test_numeric_for() {
        let src = r#"
            @stage("compute")
            export function main()
                for i = 1, 10, 2 do
                    local x = i
                end
            end
        "#;
        let module = Parser::parse(src).unwrap();
        let func = &module.functions[0];
        assert!(matches!(func.body.stmts[0], Stmt::ForNumeric { .. }));
    }

    #[test]
    fn test_if_statement() {
        let src = r#"
            @stage("fragment")
            export function main(x: f32): f32
                if x > 0 then
                    return x
                elseif x == 0 then
                    return 0
                else
                    return -x
                end
            end
        "#;
        let module = Parser::parse(src).unwrap();
        assert!(module.entry_point.is_some());
    }

    #[test]
    fn test_vector_constructor() {
        let src = r#"
            @stage("fragment")
            export function main(): vector3
                return vector3.create(1.0, 2.0, 3.0)
            end
        "#;
        let module = Parser::parse(src).unwrap();
        assert!(module.entry_point.is_some());
    }
}

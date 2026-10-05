//! This is the full ast that the parser will produce

use std::{fmt, ops::Range};

/// A span into a photon3 source file built off chumsksy's
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl From<Range<usize>> for SourceSpan {
    fn from(value: Range<usize>) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

/// A value with a `SourceSpan` location into a source file
#[derive(Debug, Clone, PartialEq)]
pub struct Located<T> {
    pub value: T,
    pub span: SourceSpan,
}

impl<T> Located<T> {
    /// Returns a new `Locaated` of a value `T`, representing
    /// its `span`/location in a source file.
    pub fn new(value: T, span: SourceSpan) -> Self {
        Self { value, span }
    }

    /// maps over the `value` of the `Located`, preserving its span.
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Located<U> {
        Located {
            value: map(self.value),
            span: self.span,
        }
    }
}

/// One qualified name,
///
/// This is essentially some namespace::name but continuing
/// for n ::'s
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedName {
    pub segments: Vec<String>,
}

/// Possible allowed type names in type positions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeName {
    Int,
    UInt,
    Float,
    Bool,
    Array,
    Tag,
    Unit,
    Void,
    Any,
    Object(QualifiedName),
}

impl fmt::Display for TypeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int => write!(formatter, "Int"),
            Self::UInt => write!(formatter, "UInt"),
            Self::Float => write!(formatter, "Float"),
            Self::Bool => write!(formatter, "Bool"),
            Self::Array => write!(formatter, "Array"),
            Self::Tag => write!(formatter, "Tag"),
            Self::Unit => write!(formatter, "Unit"),
            Self::Void => write!(formatter, "Void"),
            Self::Any => write!(formatter, "Any"),
            Self::Object(name) => write!(formatter, "{name}"),
        }
    }
}

/// One output of the parser phase,
/// this is a full module which contains a set of top level "declarations/items"
/// that can be transcribed to top level items
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub items: Vec<Located<TopLevelItem>>,
    pub namespace: QualifiedName,
}

/// All the possible top level items in a module of photon3 source code
#[derive(Debug, Clone, PartialEq)]
pub enum TopLevelItem {
    /// @namespace
    Namespace(QualifiedName),

    /// @requires
    Requires(QualifiedName),

    /// @entry
    Entry(String),

    /// @capability
    Capability(CapabilityDeclaration),

    /// @global
    Global(GlobalDeclaration),

    /// @global
    Object(ObjectDeclaration),

    /// @fn
    Function(FunctionDeclaration),
}

/// Declaration of a capability in a module
#[derive(Debug, Clone, PartialEq)]
pub struct CapabilityDeclaration {
    /// The name to bind the capability to under the current namespace
    pub name: String,

    /// The associated capability number to call with
    pub number: u64,
}

/// Declaration of a global with a type in a module
///
/// this is @global name: type
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalDeclaration {
    pub name: String,
    pub declared_type: TypeName,
}

/// Declaration of an object in a module
///
/// This is @object (param: type) etc.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDeclaration {
    pub name: String,
    pub fields: Vec<Parameter>,
}

/// A function declaration in a module
///
/// @fn name (param: type) -> type { body }
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDeclaration {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeName,
    pub body: Vec<Located<Statement>>,
}

/// One parameter (object/function) with a type declared
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub declared_type: TypeName,
}

/// All possible kinds of statement declarations in
/// Photon3.
///
/// Every body line must be a statement, which does some
/// operation on expressions (potentially) or something else.
///
/// These don't produce values
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Simple(SimpleStatement),

    Return {
        value: Option<Located<Expression>>,
    },
    If {
        condition: Located<Expression>,
        then_body: Vec<Located<Statement>>,
        else_body: Option<Vec<Located<Statement>>>,
    },
    While {
        condition: Located<Expression>,
        body: Vec<Located<Statement>>,
    },
    DoWhile {
        body: Vec<Located<Statement>>,
        condition: Located<Expression>,
    },
    For {
        initializer: Option<Located<SimpleStatement>>,
        condition: Located<Expression>,
        step: Box<Option<Located<SimpleStatement>>>,
        body: Vec<Located<Statement>>,
    },
    ForEach {
        binding: Parameter,
        array: Located<Expression>,
        body: Vec<Located<Statement>>,
    },
    Loop {
        body: Vec<Located<Statement>>,
    },
    Break,
    Continue,
    Boson3 {
        source: String,
    },
}

/// A simple statement.
///
/// This is a subset of statement allows, which are simple
/// enough to be in a for statement clause.

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleStatement {
    Let {
        name: String,
        type_annotation: Option<TypeName>,
        initializer: Located<Expression>,
    },
    Assignment {
        target: Located<Expression>,
        operator: AssignmentOperator,
        value: Located<Expression>,
    },
    Step {
        name: String,
        operator: StepOperator,
    },
    Expression(Located<Expression>),
}
/// All possible kinds of expressions, these are things
/// which actually produce some value of a type rather than
/// only do some effect
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    IntLiteral(i64),
    UIntLiteral(u64),
    FloatLiteral(f64),
    BoolLiteral(bool),
    Name(QualifiedName),
    ArrayLiteral(Vec<Located<Expression>>),
    Call {
        callee: Box<Located<Expression>>,
        arguments: Vec<Located<Expression>>,
    },
    FieldAccess {
        receiver: Box<Located<Expression>>,
        field: String,
    },
    MethodCall {
        receiver: Box<Located<Expression>>,
        method: MethodName,
        arguments: Vec<Located<Expression>>,
    },
    Index {
        array: Box<Located<Expression>>,
        index: Box<Located<Expression>>,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Located<Expression>>,
    },
    Binary {
        left: Box<Located<Expression>>,
        operator: BinaryOperator,
        right: Box<Located<Expression>>,
    },
    Conditional {
        condition: Box<Located<Expression>>,
        when_true: Box<Located<Expression>>,
        when_false: Box<Located<Expression>>,
    },
    Boson3 {
        declared_type: TypeName,
        body: String,
        body_span: SourceSpan,
    },
}

/// The name of the method we are calling on an object
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodName {
    /// Infer the name of the method, this is done through
    /// infering the namespace of the object and calling the method
    /// based on that.
    Inferred(String),
    Qualified(QualifiedName),
}

/// All possible unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Negate,
    LogicalNot,
    BitwiseNot,
}

/// All possible binary operators that exist
/// between two operands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Multiply,
    Divide,
    Remainder,
    Add,
    Subtract,
    ShiftLeft,
    ShiftRight,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    BitwiseAnd,
    BitwiseXor,
    BitwiseOr,
    LogicalAnd,
    LogicalOr,
    ArrayAppend,
}

/// All operators that can exist in the let binding
/// assignment state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentOperator {
    Assign,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    DivideAssign,
    RemainderAssign,
    ShiftLeftAssign,
    ShiftRightAssign,
    BitwiseAndAssign,
    BitwiseOrAssign,
    BitwiseXorAssign,
}

/// All step operators, these increment or decrement some value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOperator {
    Increment,
    Decrement,
}

impl fmt::Display for UnaryOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Negate => "-",
            Self::LogicalNot => "!",
            Self::BitwiseNot => "~",
        })
    }
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Remainder => "%",
            Self::Add => "+",
            Self::Subtract => "-",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::BitwiseAnd => "&",
            Self::BitwiseXor => "^",
            Self::BitwiseOr => "|",
            Self::LogicalAnd => "&&",
            Self::LogicalOr => "||",
            Self::ArrayAppend => "++",
        })
    }
}

impl fmt::Display for AssignmentOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Assign => "=",
            Self::AddAssign => "+=",
            Self::SubtractAssign => "-=",
            Self::MultiplyAssign => "*=",
            Self::DivideAssign => "/=",
            Self::RemainderAssign => "%=",
            Self::ShiftLeftAssign => "<<=",
            Self::ShiftRightAssign => ">>=",
            Self::BitwiseAndAssign => "&=",
            Self::BitwiseOrAssign => "|=",
            Self::BitwiseXorAssign => "^=",
        })
    }
}

impl fmt::Display for StepOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Increment => "++",
            Self::Decrement => "--",
        })
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = self.start;
        let end = self.end;
        write!(f, "{start}..{end}")
    }
}

impl QualifiedName {
    /// Creates a new QualifiedName from it's segments.
    pub fn new(segments: Vec<String>) -> Self {
        debug_assert!(!segments.is_empty());
        Self { segments }
    }

    /// Converts a section of text that maybe a qualified name
    /// to it's segments in a `QualifiedName`
    pub fn from_text(text: &str) -> Self {
        Self::new(text.split("::").map(str::to_owned).collect())
    }

    /// Whether this name is actually qualified or not from some namespace
    pub fn is_unqualified(&self) -> bool {
        self.segments.len() == 1
    }

    /// The last segment of the qualified name (the actual element being ref to)
    pub fn last(&self) -> &str {
        self.segments
            .last()
            .expect("QualifiedName always contains at least one segment")
    }

    /// Return the qualified name of the namespace of this qualified name.
    ///
    /// Essentially
    ///
    /// std::queue::Queue (std, queue, Queue) -> std::queue (std, queue)
    pub fn namespace(&self) -> Option<Self> {
        (self.segments.len() > 1)
            .then(|| Self::new(self.segments[..self.segments.len() - 1].to_vec()))
    }

    /// Qualify this QualifiedName from some other qualified name as an element
    /// under its namespace
    pub fn qualify_from(&self, namespace: &QualifiedName) -> Self {
        if self.is_unqualified() {
            let mut segments = namespace.segments.clone();
            segments.push(self.last().to_owned());
            Self::new(segments)
        } else {
            self.clone()
        }
    }

    /// Resolves this qualified name under the current namespace.
    ///
    /// If this name's outmost namespace is self:: this maps to the current namespace.
    /// If this name is entirely unqualified (no namespace ref), this maps to the current namespace.
    ///
    /// Otherwise this is just `name`.
    pub fn resolve(&self, current_namespace: &QualifiedName) -> QualifiedName {
        // self::
        if self
            .segments
            .first()
            .is_some_and(|segment| segment == "self")
        {
            let mut segments = current_namespace.segments.clone();
            segments.extend(self.segments.iter().skip(1).cloned());
            return QualifiedName::new(segments);
        }

        // unqualified
        if self.is_unqualified() {
            self.qualify_from(current_namespace)
        } else {
            self.clone()
        }
    }
}

impl fmt::Display for QualifiedName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.segments.join("::"))
    }
}

impl TypeName {
    /// Turns a qualified name into a type, this assumes
    /// that all non-direct Int, UInt etc. types refer to an object.
    pub fn from_qualified_name(name: QualifiedName) -> Self {
        if name.is_unqualified() {
            match name.last() {
                "Int" => return Self::Int,
                "UInt" => return Self::UInt,
                "Float" => return Self::Float,
                "Bool" => return Self::Bool,
                "Array" => return Self::Array,
                "Tag" => return Self::Tag,
                "Unit" => return Self::Unit,
                "Void" => return Self::Void,
                "Any" => return Self::Any,
                _ => {}
            }
        }

        Self::Object(name)
    }

    /// Returns the name of this type if it's an object-type.
    pub fn object_name(&self) -> Option<&QualifiedName> {
        match self {
            Self::Object(name) => Some(name),
            _ => None,
        }
    }

    /// Canonicalises this type name to the full type name
    /// including any required namespace
    ///
    /// Only real introduced type names are essentially object,
    /// so this just resolves object.
    pub fn canonicalise(&self, current_namespace: &QualifiedName) -> TypeName {
        match self {
            TypeName::Object(name) => TypeName::Object(name.resolve(current_namespace)),
            primitive => primitive.clone(),
        }
    }
}
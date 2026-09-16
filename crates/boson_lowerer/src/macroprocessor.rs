//! The actual macro processor itself,
//! this runs all the macro functions in `Boson3`

use std::{collections::HashMap, fmt, vec::IntoIter};

use crate::errors::{LoweringError, LoweringErrorKind};

/// These a special directives which should be 'ignored'
/// by the macro preprocessor and must survive it without being modified
const SAFE_DIRECTIVES: &[&str] = &["@string"];

/// This marks a parameter in a macro body
const PARAM_SIGIL: char = '$';

/// This marks an introduced name which needs to be remapped in a macro
const INTRO_SIGIL: char = '%';

/// An origin tracker,
///
/// This tracks where a certain line came from
/// in a file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    /// Index into the `@file` table.
    pub file: u64,

    /// Line within that file.
    pub line: u64,

    /// Column within that file.
    pub col: u64,

    /// Macro names this line was expanded through
    /// with the outermost first.
    pub chain: Vec<String>,
}

impl Origin {
    /// Derive the origin for lines produced by expanding `macro_name`
    /// at this location.
    ///
    /// This essentially just creates a new `Origin` with this `macro_name`
    /// appened to a cloned version of this original origin's chain.
    fn through(&self, macro_name: &str) -> Self {
        let mut chain = self.chain.clone();
        chain.push(macro_name.to_string());

        Self {
            file: self.file,
            line: self.line,
            col: self.col,
            chain,
        }
    }

    /// Returns the line number of this origin as a `usize`.
    fn line_usize(&self) -> usize {
        self.line as usize
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}", self.line)?;

        if !self.chain.is_empty() {
            write!(f, " (expanded via {})", self.chain.join(" -> "))?;
        }

        Ok(())
    }
}

/// One line of source code with location information about its
/// origin attached (for tracking while expanding)
#[derive(Debug, Clone)]
struct Line {
    origin: Origin,

    /// Verbatim source when `raw`, otherwise whitespace-joined tokens.
    text: String,

    /// Whether this line must be passed through untouched.
    raw: bool,
}

impl Line {
    /// This line of text should be cooked/expanded and not just passed through
    fn cooked(text: String, origin: Origin) -> Self {
        Self {
            origin,
            text,
            raw: false,
        }
    }

    /// This line of text is "raw" and should just pass through
    fn raw(text: String, origin: Origin) -> Self {
        Self {
            origin,
            text,
            raw: true,
        }
    }
}

/// One line of a macro body.
#[derive(Debug, Clone)]
struct BodyLine {
    text: String,
    raw: bool,
}

/// A @macro definition, this is essentially
/// some textual replacement form with some extra
/// logic for hygiene
#[derive(Debug)]
struct Macro {
    /// Parameter names introduced by the macro,
    /// with the dedicated macro sigil stripped.
    params: Vec<String>,

    /// The body lines of the macro
    body: Vec<BodyLine>,
}

/// An argument provided during the invocation of a macro
#[derive(Debug, Clone)]
enum MacroArg {
    /// A simple one-line token
    Token(String),

    /// A block of lines
    Block(Vec<Line>),
}

/// The macro expander itself
///
/// This takes some source input and outputs a macro-expanded
/// version of the source with all `@macro` directives removed.
#[derive(Debug)]
pub struct MacroExpander<'source> {
    /// All collected macro definitions by name.
    macros: HashMap<String, Macro>,

    /// All the filenames for later mapping purposes
    filenames: HashMap<u64, String>,

    /// The input source file we are expanding.
    source: &'source str,

    /// The expanded output lines.
    out: Vec<Line>,

    /// Monotonic counter used to make hygiene function with unique names.
    next_id: u64,

    /// How many expansions have run so far
    total_expansions: u64,

    /// The maximum allowed number of total expansions to permit
    /// before erroring
    max_total_expansions: u64,

    /// Number of passes run so far.
    passes: u64,

    /// Ceiling on `passes`.
    max_passes: u64,
}

impl<'source> MacroExpander<'source> {
    /// Creates a new `Boson3` macro expander, this is responsible
    /// for expanding the `Boson3` macros out
    ///
    /// `max_passes` bounds macro nesting depth
    ///
    /// `max_total_expansions` bounds overall work
    pub fn new(source: &'source str, max_passes: u64, max_total_expansions: u64) -> Self {
        Self {
            macros: HashMap::new(),
            filenames: HashMap::new(),
            source,
            out: Vec::new(),
            next_id: 0,
            total_expansions: 0,
            max_total_expansions,
            passes: 0,
            max_passes,
        }
    }

    /// Expand a complete `Boson3` source file's macros
    pub fn expand(mut self) -> Result<String, LoweringError> {
        self.collect()?;

        // Expand all the macros
        self.expand_until_complete()?;

        // Resolve all of the scopes in the file
        let resolved = resolve_scopes(std::mem::take(&mut self.out))?;

        // Turn lines back into a Vec<String>
        let mut text = resolved
            .into_iter()
            .map(|line| line.text)
            .collect::<Vec<String>>()
            .join("\n");

        text.push('\n');
        Ok(text)
    }

    /// Collects every @macro ... @end definition out of the source into
    /// the macro table
    ///
    /// This will then be used for macro expansion later during the "recursive" phase
    fn collect(&mut self) -> Result<(), LoweringError> {
        let mut lines = self.source.lines().enumerate();
        let mut collected_out_buf = Vec::new();

        // Set by @loc, applies to every following line until the next @loc.
        let mut current_loc = None;

        while let Some((index, source_line)) = lines.next() {
            let line_number = index + 1;

            // Get the current origin of the line
            let origin = current_loc.clone().unwrap_or(Origin {
                line: line_number as u64,
                ..Origin::default()
            });

            let untouched = source_line.trim();

            // Make sure it isnt a "safe directive" (one that survives through
            // the macro expansino pass)
            if is_safe_directive(untouched) {
                collected_out_buf.push(Line::raw(untouched.to_string(), origin));
                continue;
            }

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(untouched).trim();

            if line.is_empty() {
                continue;
            }

            let tokens_string = tokenize(line);
            let tokens = tokens_string.iter().map(String::as_str).collect::<Vec<_>>();

            match tokens.as_slice() {
                // @file declaration
                ["@file", file_idx, path @ ..] => {
                    // Parse the file index for the file table
                    let file_index = parse_u64(line_number, file_idx)?;

                    // Path in the mapping for later
                    self.filenames.insert(file_index, path.join(" "));
                    collected_out_buf.push(Line::cooked(line.to_string(), origin));
                }

                // @loc <file_idx> <line> <col>
                ["@loc", file_idx, src_line, src_col] => {
                    let file = parse_u64(line_number, file_idx)?;
                    let src_line = parse_u64(line_number, src_line)?;
                    let src_col = parse_u64(line_number, src_col)?;

                    // Make sure this file is defined in the set of filenames at this point.
                    // (these should really be at the start anyway)
                    if !self.filenames.contains_key(&file) {
                        return Err(LoweringErrorKind::FileIndexNotDefined { index: file }
                            .with_line(line_number));
                    }

                    // The current location will then be this @loc which we track through the lines
                    // to make sure they have the correct loc at the end.
                    let loc = Origin {
                        file,
                        line: src_line,
                        col: src_col,
                        chain: Vec::new(),
                    };

                    collected_out_buf.push(Line::cooked(line.to_string(), loc.clone()));
                    current_loc = Some(loc);
                }

                // @macro <name> (<param>, <param>, ...)
                ["@macro", name, params @ ..] => {
                    let params = parse_params(line_number, &tokens, params)?;
                    let body = collect_body(&mut lines, line_number, name)?;

                    // Validate the macro body is valid.
                    validate_body(&body, &params, name, line_number)?;

                    let macro_def = Macro { params, body };

                    // Prevent duplicates (else what is the order?)
                    if self.macros.insert(name.to_string(), macro_def).is_some() {
                        return Err(LoweringErrorKind::DuplicateMacro {
                            name: name.to_string(),
                        }
                        .with_line(line_number));
                    }
                }

                // An @end with no @macro that it closes
                // This is not valid for this directive
                ["@end", ..] => {
                    return Err(LoweringErrorKind::InvalidArgument {
                        expected: "a matching @macro".to_string(),
                        got: "@end".to_string(),
                    }
                    .with_line(line_number));
                }

                // Non-macro definitions, leave for the actual expansion pass
                other => {
                    collected_out_buf.push(Line::cooked(other.join(" "), origin));
                }
            }
        }

        // Update out because now we've collected macros we now
        // then repeatedly expand on output
        self.out = collected_out_buf;

        Ok(())
    }

    /// Continuously runs the expansion of macros until there
    /// are no more expansions or an error occured
    fn expand_until_complete(&mut self) -> Result<(), LoweringError> {
        loop {
            // Track total number of passes
            self.passes += 1;

            if self.passes > self.max_passes {
                return Err(LoweringErrorKind::PassLimit {
                    passes: self.max_passes,
                }
                .with_line(0));
            }

            // A macro wasn't seen this time around, so we're done!
            if !self.expand_lines()? {
                return Ok(());
            }
        }
    }

    /// Runs the expansion of macros on the current self.out
    ///
    /// This returns whether or not a macro was seen this pass.
    fn expand_lines(&mut self) -> Result<bool, LoweringError> {
        // The current source of our expansion pass
        let current_expansion_source = std::mem::take(&mut self.out);
        let mut lines = current_expansion_source.into_iter();

        // The destination of all expanded things from this pass
        let mut current_expansion_out = vec![];

        // Whether or not a macro invocation was found this pass
        let mut found_macro_invocation = false;

        while let Some(line) = lines.next() {
            if line.raw {
                current_expansion_out.push(line);
                continue;
            }

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let tokens = tokenize(&line.text);

            if tokens.is_empty() {
                continue;
            }

            let borrowed = tokens.iter().map(String::as_str).collect::<Vec<_>>();

            // !<name> <arg> { <arg> } ...
            let Some(invocation) = borrowed.first() else {
                current_expansion_out.push(line);
                continue;
            };

            // Grab the name of the macro we are
            let Some(name) = invocation.strip_prefix('!') else {
                current_expansion_out.push(line);
                continue;
            };

            found_macro_invocation = true;

            // This macro wasn't found
            if !self.macros.contains_key(name) {
                return Err(LoweringErrorKind::UndefinedMacro {
                    name: name.to_string(),
                }
                .with_line(line.origin.line_usize()));
            }

            self.next_id += 1;

            // Ensure we are under the total expansion limit
            self.total_expansions += 1;

            if self.total_expansions > self.max_total_expansions {
                return Err(LoweringErrorKind::ExpansionTotalLimit {
                    name: name.to_string(),
                }
                .with_line(line.origin.line_usize()));
            }

            let id = self.next_id;

            // Whether or not we should emit a macro for this (essentially if theres a actual file)
            let emit_loc = self.filenames.contains_key(&line.origin.file);

            let macro_def = &self.macros[name];

            let rest = borrowed[1..].iter().map(|tok| tok.to_string()).collect();

            // Parse all of the arguments to this macro
            let mut cursor = ArgCursor::new(&mut lines, line.origin.clone(), rest);
            let args = cursor.parse_args(macro_def.params.len(), name)?;

            // Expand the actual macro here
            let expanded = expand_macro(macro_def, &args, id, name, &line.origin, emit_loc)?;

            current_expansion_out.extend(expanded);
        }

        self.out = current_expansion_out;
        Ok(found_macro_invocation)
    }
}

/// Strip a line comment from a line
fn strip_comment(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else {
        line
    }
}

/// Whether a line is a directive which must remain
/// unmodified through the macro expansion pass such as
/// @string bcz we dont want to explode the string literal
fn is_safe_directive(line: &str) -> bool {
    let Some(first) = line.split_whitespace().next() else {
        return false;
    };

    SAFE_DIRECTIVES.contains(&first)
}

/// Tokenises a line into a Vec<String> which are the tokens of this line
///
/// This will strip comments, split whitespace and then push braces that
/// are stuck onto tokens as seperate tokens.
fn tokenize(line: &str) -> Vec<String> {
    let mut out = Vec::new();

    for token in strip_comment(line).split_whitespace() {
        push_split_braces(token, &mut out);
    }

    out
}

/// Peel leading and trailing braces off a token into separate tokens.
fn push_split_braces(token: &str, out: &mut Vec<String>) {
    let mut rest = token;

    // If the first character is { or } then peel it off
    while let Some(first) = rest.chars().next() {
        if first != '{' && first != '}' {
            break;
        }

        out.push(first.to_string());
        rest = &rest[first.len_utf8()..];
    }

    let mut trailing = Vec::new();

    // If the last character is { or }, peel it off.
    while let Some(last) = rest.chars().last() {
        if last != '{' && last != '}' {
            break;
        }

        trailing.push(last.to_string());
        rest = &rest[..rest.len() - last.len_utf8()];
    }

    if !rest.is_empty() {
        out.push(rest.to_string());
    }

    out.extend(trailing.into_iter().rev());
}

/// Parse a u64 from a string token
///
/// # Errors
///
/// This will error with a `LoweringError::InvalidArgument` if the
/// string token cannot be successfully converted to a `u64`
fn parse_u64(line: usize, token: &str) -> Result<u64, LoweringError> {
    token.parse::<u64>().map_err(|_| {
        LoweringErrorKind::InvalidArgument {
            expected: "64-Bit Unsigned Integer".to_string(),
            got: token.to_string(),
        }
        .with_line(line)
    })
}

/// Parse `(<param>, <param>, ...)` off a @macro signature.
///
/// Both `(x, y)` and `($x, $y)` are accepted and normalise to bare names.
fn parse_params(
    line_number: usize,
    tokens: &[&str],
    params: &[&str],
) -> Result<Vec<String>, LoweringError> {
    let string_params = params.join(" ");

    // The parameters must start with ( and ) as per the macro directive
    if !string_params.starts_with('(') || !string_params.ends_with(')') {
        return Err(LoweringErrorKind::InvalidArgument {
            expected: "@macro <name> (<param>, <param>, ...)".to_string(),
            got: tokens.join(" "),
        }
        .with_line(line_number));
    }

    let inner = &string_params[1..string_params.len() - 1];

    // Get all param names, which may start with the PARAM_SIGIL (also used in body)
    let mut param_names = inner
        .split(',')
        .map(|split_str| {
            let trimmed = split_str.trim();
            trimmed
                .strip_prefix(PARAM_SIGIL)
                .unwrap_or(trimmed)
                .to_string()
        })
        .collect::<Vec<_>>();

    // `()` should be no-params, similar to functions we clear param names
    if param_names.len() == 1 && param_names[0].is_empty() {
        param_names.clear();
    }

    Ok(param_names)
}

/// Collect the body lines of a macro until the matching @end.
fn collect_body(
    lines: &mut std::iter::Enumerate<std::str::Lines<'_>>,
    line_number: usize,
    name: &str,
) -> Result<Vec<BodyLine>, LoweringError> {
    let mut body = Vec::new();
    let mut terminated = false;

    for (body_index, body_line) in lines.by_ref() {
        let untouched = body_line.trim();

        // If this directive should be safe, do not touch it !!!!
        if is_safe_directive(untouched) {
            body.push(BodyLine {
                text: untouched.to_string(),
                raw: true,
            });
            continue;
        }

        let body_line = strip_comment(untouched).trim();

        if body_line == "@end" {
            terminated = true;
            break;
        }

        // Nested macro definitions aren't allowed!
        if body_line.starts_with("@macro") {
            return Err(LoweringErrorKind::NestedMacroDefinition.with_line(body_index + 1));
        }

        if !body_line.is_empty() {
            body.push(BodyLine {
                text: body_line.to_string(),
                raw: false,
            });
        }
    }

    if !terminated {
        return Err(LoweringErrorKind::UnterminatedMacro {
            name: name.to_string(),
        }
        .with_line(line_number));
    }

    Ok(body)
}

/// Catch `$typo` at definition time rather than emitting a bare `$typo`
/// token into the Quark3 stream.
fn validate_body(
    body: &[BodyLine],
    params: &[String],
    name: &str,
    line_number: usize,
) -> Result<(), LoweringError> {
    for body_line in body {
        // We shouldn't tokenise this one, since its raw and pass through
        if body_line.raw {
            continue;
        }

        for token in tokenize(&body_line.text) {
            // Labels can be a param which ends with `:`
            let stem = token.strip_suffix(':').unwrap_or(&token);

            let Some(param) = stem.strip_prefix(PARAM_SIGIL) else {
                continue;
            };

            // Ensure that if its a param (PARAM_SIGIL), it must exist in params,
            // else this is a `$typo` or just something the user forgot to define.
            if !params.iter().any(|candidate| candidate == param) {
                return Err(LoweringErrorKind::UndefinedMacroParameter {
                    name: name.to_string(),
                    param: param.to_string(),
                }
                .with_line(line_number));
            }
        }
    }

    Ok(())
}

/// One argument that was pulled off
enum Chunk {
    /// A normal token.
    Token(String),

    /// A raw directive line, which cannot be split into tokens.
    RawLine(Line),

    /// A new source line was reached.
    LineBreak,

    /// No more input.
    Eof,
}

/// Cursor over the a token stream for arguments to a macro invocation
/// which may run past the end of the invocation line and onto following lines.
struct ArgCursor<'a> {
    // The lines we are a cursor over
    lines: &'a mut IntoIter<Line>,

    /// Remaining tokens on the line currently being consumed.
    current: Vec<String>,

    /// A raw line waiting to be handed to the caller.
    pending_raw: Option<Line>,

    /// Origin of the line currently being consumed.
    origin: Origin,
}

impl<'a> ArgCursor<'a> {
    /// Returns a new `ArgCursor` over the lines which will parse all of the macros
    fn new(lines: &'a mut IntoIter<Line>, origin: Origin, current: Vec<String>) -> Self {
        Self {
            lines,
            current,
            pending_raw: None,
            origin,
        }
    }

    /// Whether the invocation's own line has been fully consumed.
    fn line_exhausted(&self) -> bool {
        self.current.is_empty() && self.pending_raw.is_none()
    }

    // This will pull the next argument "Chunk" off the cursor, or advancing
    // to the next line if the current line is empty (LineBreak at the end).
    fn take(&mut self) -> Chunk {
        if let Some(line) = self.pending_raw.take() {
            return Chunk::RawLine(line);
        }

        if !self.current.is_empty() {
            return Chunk::Token(self.current.remove(0));
        }

        loop {
            let Some(line) = self.lines.next() else {
                return Chunk::Eof;
            };

            self.origin = line.origin.clone();

            if line.raw {
                self.pending_raw = Some(line);
                return Chunk::LineBreak;
            }

            let tokens = tokenize(&line.text);

            // @loc lines are metadata, never take these as an argument
            if tokens.first().map(String::as_str) == Some("@loc") {
                continue;
            }

            if tokens.is_empty() {
                continue;
            }

            self.current = tokens;
            return Chunk::LineBreak;
        }
    }

    /// Parses all the arguments to a macro invocation.
    ///
    /// `arg_count` is the total number of arguments to the macro which is
    /// attempted to be matched.
    fn parse_args(&mut self, arg_count: usize, name: &str) -> Result<Vec<MacroArg>, LoweringError> {
        let mut args = Vec::with_capacity(arg_count);

        while args.len() < arg_count {
            match self.take() {
                Chunk::LineBreak => continue,

                // If we hit EOF before getting all the required arguments, then
                // we must be missing some.
                Chunk::Eof => {
                    return Err(LoweringErrorKind::MissingMacroArguments {
                        name: name.to_string(),
                        expected: arg_count as u64,
                        got: args.len() as u64,
                    }
                    .with_line(self.origin.line_usize()));
                }

                // A raw directive can only appear inside a block argument.
                Chunk::RawLine(_) => {
                    return Err(LoweringErrorKind::RawDirectiveAsArgument {
                        name: name.to_string(),
                    }
                    .with_line(self.origin.line_usize()));
                }

                // A block, otherwise a normal token.
                Chunk::Token(token) => match token.as_str() {
                    "{" => args.push(MacroArg::Block(self.parse_block(name)?)),

                    "}" => {
                        return Err(LoweringErrorKind::InvalidArgument {
                            expected: format!("an argument to !{name}"),
                            got: "}".to_string(),
                        }
                        .with_line(self.origin.line_usize()));
                    }

                    _ => args.push(MacroArg::Token(token)),
                },
            }
        }

        // Leftover tokens after the final argument on the same line are not permitted
        if !self.line_exhausted() {
            return Err(LoweringErrorKind::MacroInvocationLeftoverTokens {
                name: name.to_string(),
            }
            .with_line(self.origin.line_usize()));
        }

        Ok(args)
    }

    /// Collects the lines of a `{ }` block argument to a macro invocation
    ///
    /// These can span lines.
    fn parse_block(&mut self, name: &str) -> Result<Vec<Line>, LoweringError> {
        let mut block = Vec::new();
        let mut depth = 1u64;

        // Tokens gathered for the block line currently being built.
        let mut building = Vec::new();
        let mut building_origin = self.origin.clone();

        loop {
            match self.take() {
                Chunk::LineBreak => {
                    if !building.is_empty() {
                        block.push(Line::cooked(building.join(" "), building_origin.clone()));
                        building.clear();
                    }

                    building_origin = self.origin.clone();
                }

                Chunk::RawLine(line) => {
                    if !building.is_empty() {
                        block.push(Line::cooked(building.join(" "), building_origin.clone()));
                        building.clear();
                    }

                    block.push(line);
                }

                // The end of file was hit before we found the `}`
                Chunk::Eof => {
                    return Err(LoweringErrorKind::UnterminatedBlock {
                        name: name.to_string(),
                    }
                    .with_line(self.origin.line_usize()));
                }

                Chunk::Token(token) => match token.as_str() {
                    // Keep expanding sub-blocks until we close the original..
                    "{" => {
                        depth += 1;
                        building.push(token);
                    }

                    "}" => {
                        depth -= 1;

                        // The original block we are trying to parse is now closed!
                        if depth == 0 {
                            if !building.is_empty() {
                                block.push(Line::cooked(building.join(" "), building_origin));
                            }

                            return Ok(block);
                        }

                        // Else we've just got a nested block
                        building.push(token);
                    }

                    _ => building.push(token),
                },
            }
        }
    }
}

/// Produce one expansion of a macro's output block
fn expand_macro(
    macro_def: &Macro,
    args: &[MacroArg],
    id: u64,
    macro_name: &str,
    origin: &Origin,
    emit_loc: bool,
) -> Result<Vec<Line>, LoweringError> {
    let child = origin.through(macro_name);
    let mut out = Vec::new();

    // Point debug info at the invocation site
    // so the user can know
    if emit_loc {
        out.push(Line::cooked(
            format!("@loc {} {} {}", origin.file, origin.line, origin.col),
            child.clone(),
        ));
    }

    for body_line in &macro_def.body {
        // Substitute params into raw directive
        if body_line.raw {
            out.push(Line::raw(
                substitute_raw(&body_line.text, macro_def, args),
                child.clone(),
            ));
            continue;
        }

        let tokens = tokenize(&body_line.text);

        if tokens.is_empty() {
            continue;
        }

        // A parameter alone on a line splices the whole argument in here.
        if let [lone] = tokens.as_slice()
            && let Some(param) = lone.strip_prefix(PARAM_SIGIL)
            && let Some(index) = macro_def.params.iter().position(|p| p == param)
        {
            match &args[index] {
                MacroArg::Token(token) => {
                    out.push(Line::cooked(token.clone(), child.clone()));
                }
                MacroArg::Block(lines) => out.extend(lines.iter().cloned()),
            }
            continue;
        }

        let mut rebuilt = Vec::with_capacity(tokens.len());

        for token in &tokens {
            // A label definition matches on its name, we keep the `:`
            let colon = token.ends_with(':');
            let stem = if colon {
                &token[..token.len() - 1]
            } else {
                token.as_str()
            };

            if let Some(param) = stem.strip_prefix(PARAM_SIGIL) {
                let Some(index) = macro_def.params.iter().position(|p| p == param) else {
                    return Err(LoweringErrorKind::UndefinedMacroParameter {
                        name: macro_name.to_string(),
                        param: param.to_string(),
                    }
                    .with_line(origin.line_usize()));
                };

                // Inline expansion only permits parameters of type `Token`
                // as blocks dont really make sense to be inline here.
                let MacroArg::Token(argument) = &args[index] else {
                    return Err(LoweringErrorKind::BlockArgumentInline {
                        name: macro_name.to_string(),
                        param: param.to_string(),
                    }
                    .with_line(origin.line_usize()));
                };

                rebuilt.push(if colon {
                    format!("{argument}:")
                } else {
                    argument.clone()
                });

            // If this starts with the "INTRO_SIGIL" then we need to specially add an id for this macro
            // to make it unique across the file
            } else if let Some(introduced) = stem.strip_prefix(INTRO_SIGIL) {
                rebuilt.push(if colon {
                    format!("{introduced}#macro_{id}:")
                } else {
                    format!("{introduced}#macro_{id}")
                });
            } else {
                rebuilt.push(token.clone());
            }
        }

        out.push(Line::cooked(rebuilt.join(" "), child.clone()));
    }

    Ok(out)
}

/// Substitute `$param` inside a raw line of a directive
fn substitute_raw(text: &str, macro_def: &Macro, args: &[MacroArg]) -> String {
    let mut order = (0..macro_def.params.len()).collect::<Vec<_>>();

    // We reverse here because we want to match longest first then shortest.
    order.sort_by_key(|&index| std::cmp::Reverse(macro_def.params[index].len()));

    let mut out = text.to_string();

    for index in order {
        if let MacroArg::Token(value) = &args[index] {
            let needle = format!("{PARAM_SIGIL}{}", macro_def.params[index]);
            out = out.replace(&needle, value);
        }
    }

    out
}

/// A scope is a special label pair formed from a "break"/"continue"
/// label somewhere in source code that is inserted.
#[derive(Debug)]
struct Scope {
    brk: Option<String>,
    cont: Option<String>,
}

/// Placeholder for a scope having no target of this kind
const NO_TARGET: &str = "-";

/// Resolve `@break` / `@continue` against `@scope_push` / `@scope_pop`s
///
/// This is essentially a special resolving that matches these scope markers together
/// and the special @break/@continue directives
fn resolve_scopes(lines: Vec<Line>) -> Result<Vec<Line>, LoweringError> {
    let mut stack = Vec::new();
    let mut out = Vec::new();

    for line in lines {
        if line.raw {
            out.push(line);
            continue;
        }

        let tokens = tokenize(&line.text);
        let borrowed = tokens.iter().map(String::as_str).collect::<Vec<_>>();

        match borrowed.as_slice() {
            [] => continue,

            // A new scope
            ["@scope_push", brk, cont] => {
                stack.push(Scope {
                    brk: optional_target(brk),
                    cont: optional_target(cont),
                });
            }

            // Pops the previous scope (or error if there were none found)
            ["@scope_pop"] => {
                if stack.pop().is_none() {
                    return Err(
                        LoweringErrorKind::UnbalancedScope.with_line(line.origin.line_usize())
                    );
                }
            }

            // Handle the @break/@continue directives
            [directive @ ("@break" | "@continue"), rest @ ..] => {
                let depth = match rest {
                    // Break out of the previous scope
                    [] => 0usize,

                    // Else continue/break out a certain "depth" of scope
                    // in the current position
                    [count] => parse_u64(line.origin.line_usize(), count)? as usize,
                    _ => {
                        return Err(LoweringErrorKind::InvalidArgument {
                            expected: format!("{directive} [<depth>]"),
                            got: borrowed.join(" "),
                        }
                        .with_line(line.origin.line_usize()));
                    }
                };

                // Get the specific scope we are handling with this directive
                let Some(index) = stack.len().checked_sub(depth + 1) else {
                    return Err(LoweringErrorKind::ScopeDirectiveOutsideScope {
                        directive: directive.to_string(),
                    }
                    .with_line(line.origin.line_usize()));
                };

                let scope = &stack[index];

                // Whether we are targetting the "brk" or "cnt" here.
                let target = if *directive == "@break" {
                    scope.brk.as_ref()
                } else {
                    scope.cont.as_ref()
                };

                let Some(target) = target else {
                    return Err(LoweringErrorKind::ScopeTargetUnavailable {
                        directive: directive.to_string(),
                    }
                    .with_line(line.origin.line_usize()));
                };

                // This is essentially equal to us just jumping to that specific brk/cnt target.
                out.push(Line::cooked(format!("jump {target}"), line.origin.clone()));
            }

            _ => out.push(line),
        }
    }

    if stack.last().is_some() {
        return Err(LoweringErrorKind::UnbalancedScope.with_line(0));
    }

    Ok(out)
}

/// Parses a optional target for the @scope directives
///
/// The "-" NO_TARGET refers to None here, otherwise some.
fn optional_target(token: &str) -> Option<String> {
    if token == NO_TARGET {
        None
    } else {
        Some(token.to_string())
    }
}

//! The actual macro & scope processor itself,
//! this runs all the macro & scope functions in `Boson3`

use std::{collections::HashMap, fmt, vec::IntoIter};

use regex::Regex;

use crate::errors::{LoweringError, LoweringErrorKind};

/// These a special directives which should be 'ignored'
/// by this preprocessor and must survive it without being modified,
/// as else bad things will occur
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
    /// Derive a new origin for lines produced by expanding `macro_name`
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

/// One line of a match in a macro
/// These define requirements at compile time for macro matches
/// of the param values
#[derive(Debug)]
struct MatchReq {
    param: String,
    match_req: Regex,
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

    /// Match lines of the macro for ensuring params meet some regex
    match_req: Vec<MatchReq>,
}

/// An argument provided during the invocation of a macro
#[derive(Debug, Clone)]
enum MacroArg {
    /// A simple one-line token
    Token(String),

    /// A block of lines
    Block(Vec<Line>),
}

/// The macro & scope expander itself
///
/// This takes some source input and outputs a macro-expanded
/// version of the source with all `@macro` directives removed.
///
/// (also all the @scope/@break/@continue directives)
#[derive(Debug)]
pub struct MacroScopeExpander<'source> {
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

impl<'source> MacroScopeExpander<'source> {
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

            let tokens_string = tokenise(line);
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
                    let (body, matches) = collect_body(&mut lines, line_number, name)?;

                    // Validate the macro body is valid.
                    validate_body(&body, &params, name, line_number)?;

                    // All matches must be valid params
                    for match_req in &matches {
                        if !params.contains(&match_req.param) {
                            return Err(LoweringErrorKind::MatchPatternInvalidParam {
                                param_name: match_req.param.clone(),
                            }
                            .with_line(line_number));
                        }
                    }

                    let macro_def = Macro {
                        params,
                        body,
                        match_req: matches,
                    };

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
            let tokens = tokenise(&line.text);

            if tokens.is_empty() {
                continue;
            }

            let borrowed = tokens.iter().map(String::as_str).collect::<Vec<_>>();

            // !<name> <arg> { <arg> } ...
            let Some(invocation) = borrowed.first() else {
                current_expansion_out.push(line);
                continue;
            };

            let Some(name) = invocation.strip_prefix('!') else {
                current_expansion_out.push(line);
                continue;
            };

            // We know a macro invocation must exist because it matches the syntax
            found_macro_invocation = true;

            // This macro wasn't found in the defined macros at time of expansion
            // so it must not be defined.
            if !self.macros.contains_key(name) {
                return Err(LoweringErrorKind::UndefinedMacro {
                    name: name.to_string(),
                }
                .with_origin(line.origin));
            }

            self.next_id += 1;

            // Ensure we are under the total expansion limit
            self.total_expansions += 1;

            if self.total_expansions > self.max_total_expansions {
                return Err(LoweringErrorKind::ExpansionTotalLimit {
                    name: name.to_string(),
                }
                .with_origin(line.origin));
            }

            let id = self.next_id;

            // Get the file name if it exists.
            let file_name = self.filenames.get(&line.origin.file).map(String::as_str);

            let macro_def = &self.macros[name];

            let rest = borrowed[1..].iter().map(|tok| tok.to_string()).collect();

            // Parse all of the arguments to this macro
            let mut cursor = ArgCursor::new(&mut lines, line.origin.clone(), rest);
            let args = cursor.parse_args(macro_def.params.len(), name)?;

            // Expand the actual macro
            let expanded = expand_macro(macro_def, &args, id, name, &line.origin, file_name)?;

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
fn tokenise(line: &str) -> Vec<String> {
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
/// Both `(x, y)` and `($x, $y)` are accepted and normalise to bare names
/// bcz the old syntax is (x, y) but it may make sense too for a user to use $
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
) -> Result<(Vec<BodyLine>, Vec<MatchReq>), LoweringError> {
    let mut body = Vec::new();
    let mut match_reqs = Vec::new();
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

        // Match patterns with regex
        if body_line.starts_with("@matches") {
            let body_split = body_line.splitn(3, char::is_whitespace).collect::<Vec<_>>();

            // Length of directive args must be at least 3 (@matches param match_pattern)
            if body_split.len() < 3 {
                return Err(LoweringErrorKind::InvalidMatchPattern.with_line(body_index + 1));
            }

            // Second argument will then be the param
            let param = body_split[1].to_string();

            // Next we need to extract the pattern which is the remaining part
            let pattern = Regex::new(body_split[2]).map_err(|err| {
                LoweringErrorKind::MatchPatternRegexFailCompile { regex_error: err }
                    .with_line(body_index + 1)
            })?;

            // Now build the @matches
            match_reqs.push(MatchReq {
                param,
                match_req: pattern,
            });

            continue;
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

    Ok((body, match_reqs))
}

/// Validate all the used macro parameters in its body actually
/// are a parameter to the macro
///
/// E.g $typo is actually a valid parameter to the @macro
fn validate_body(
    body: &[BodyLine],
    params: &[String],
    name: &str,
    line_number: usize,
) -> Result<(), LoweringError> {
    for body_line in body {
        // We shouldn't tokenise this one, since its raw and pass through regardless
        // else we may accidentally over-zealously match
        if body_line.raw {
            continue;
        }

        for token in tokenise(&body_line.text) {
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

            let tokens = tokenise(&line.text);

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
    file_name: Option<&str>,
) -> Result<Vec<Line>, LoweringError> {
    let child = origin.through(macro_name);
    let mut out = Vec::new();

    // Context for the macro expansion LOC's
    let mut context = match file_name {
        Some(file_name) => format!(
            "macro {macro_name} invoked in {file_name} at line {}",
            origin.line
        ),
        None => format!("macro {macro_name} invoked at line {}", origin.line),
    };

    if child.chain.len() > 1 {
        context.push_str(&format!(" (expanded via {})", child.chain.join(" -> ")));
    }

    // Point debug info at the invocation site
    // so the user can know
    if file_name.is_some() {
        out.push(Line::cooked(
            format!(
                "@loc {} {} {} {context}",
                origin.file, origin.line, origin.col
            ),
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

        let tokens = tokenise(&body_line.text);

        if tokens.is_empty() {
            continue;
        }

        // Preserve macro expansion context on LOC lines throughout expansion
        if tokens.first().map(String::as_str) == Some("@loc") {
            out.push(Line::cooked(
                format!("{} {context}", tokens.join(" ")),
                child.clone(),
            ));
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

        // Validate the macro args against matches
        for match_req in &macro_def.match_req {
            // Corresponding index in the actual params
            let Some(index) = macro_def.params.iter().position(|p| match_req.param.eq(p)) else {
                return Err(LoweringErrorKind::UndefinedMacroParameter {
                    name: macro_name.to_string(),
                    param: match_req.param.to_string(),
                }
                .with_origin(child));
            };

            // Grab the corresponding arg
            let arg_matches = match &args[index] {
                MacroArg::Token(token_arg) => match_req.match_req.is_match(token_arg),
                MacroArg::Block(lines) => lines
                    .iter()
                    .all(|line| match_req.match_req.is_match(&line.text)),
            };

            if !arg_matches {
                return Err(LoweringErrorKind::MatchPatternFailed {
                    param_name: match_req.param.clone(),
                    match_req: match_req.match_req.clone(),
                }
                .with_origin(child));
            }
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
                    .with_origin(child));
                };

                // Inline expansion only permits parameters of type `Token`
                // as blocks dont really make sense to be inline here.
                let MacroArg::Token(argument) = &args[index] else {
                    return Err(LoweringErrorKind::BlockArgumentInline {
                        name: macro_name.to_string(),
                        param: param.to_string(),
                    }
                    .with_origin(child));
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
///
/// Essentially just a driver for loops lol but i mean you can technically
/// use it for other things, its just two labels that can be jumped to specially
/// through the @break and @continue directives.
#[derive(Debug)]
struct Scope {
    brk: Option<String>,
    cont: Option<String>,
}

/// Placeholder for a scope having no target of this kind
///
/// Use this in the scope push directive for declaring there is no
/// label of this kind
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

        // get rid of whitespace and turn into tokens
        let tokens = tokenise(&line.text);
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
                    return Err(LoweringErrorKind::UnbalancedScope.with_origin(line.origin));
                }
            }

            // Handle the @break/@continue directives
            [directive @ ("@break" | "@continue"), rest @ ..] => {
                let target = if rest.is_empty() {
                    // If there is no explicit depth then try
                    // find the first matching scope outwards
                    if stack.is_empty() {
                        return Err(LoweringErrorKind::ScopeDirectiveOutsideScope {
                            directive: directive.to_string(),
                        }
                        .with_origin(line.origin));
                    }

                    // Whether we are targetting the "brk" or "cnt" here.
                    // find the first provided one out
                    stack.iter().rev().find_map(|scope| {
                        if *directive == "@break" {
                            scope.brk.as_ref()
                        } else {
                            scope.cont.as_ref()
                        }
                    })
                } else {
                    // We have an explicit depth (hopefully) passed in, which we are breaking out of.
                    let depth = match rest {
                        [count] => parse_u64(line.origin.line_usize(), count)? as usize,
                        _ => {
                            return Err(LoweringErrorKind::InvalidArgument {
                                expected: format!("{directive} [<depth>]"),
                                got: borrowed.join(" "),
                            }
                            .with_origin(line.origin));
                        }
                    };

                    let scope = stack.iter().rev().nth(depth).ok_or_else(|| {
                        LoweringErrorKind::ScopeDirectiveOutsideScope {
                            directive: directive.to_string(),
                        }
                        .with_origin(line.origin.clone())
                    })?;

                    if *directive == "@break" {
                        scope.brk.as_ref()
                    } else {
                        scope.cont.as_ref()
                    }
                };

                // Cant find a target in the end!
                let Some(target) = target else {
                    return Err(LoweringErrorKind::ScopeTargetUnavailable {
                        directive: directive.to_string(),
                    }
                    .with_origin(line.origin));
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

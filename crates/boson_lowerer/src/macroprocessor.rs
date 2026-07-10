//! The actual macro processor itself,
//! this runs all the macro functions in `Boson3`

use std::{collections::HashMap, iter::Enumerate, vec::IntoIter};

use crate::errors::{LoweringError, LoweringErrorKind};

/// A @macro definition, this is essentially
/// some textual replacement form with some extra
/// logic for hygiene
#[derive(Debug)]
struct Macro {
    /// The parameter names of the macro
    params: Vec<String>,

    /// The body lines of the macro
    body: Vec<String>,

    /// The names introduced by the body which need to
    /// be remapped such as in labels and @locals
    introduced: Vec<String>,
}

/// An argument provided during the invocation of a macro
#[derive(Debug, Clone)]
enum MacroArg {
    /// A simple one-line token
    Token(String),

    /// A block of lines
    Block(Vec<String>),
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
    out: Vec<String>,

    /// How many expansions have run so far
    expansions: u64,

    /// The maximum allowed number of expansions to permit
    /// before erroring
    max_expansions: u64,
}

impl<'source> MacroExpander<'source> {
    /// Creates a new `Boson3` macro expander, this is responsible
    /// for expanding the `Boson3` macros out
    pub fn new(source: &'source str, max_expansions: u64) -> Self {
        Self {
            macros: HashMap::new(),
            source,
            out: Vec::new(),
            expansions: 0,
            max_expansions,
            filenames: HashMap::new(),
        }
    }

    /// Expand a complete `Boson3` source file's macros
    pub fn expand(mut self) -> Result<String, LoweringError> {
        // Collect all of the @macro definitions
        self.collect()?;
        self.expand_until_complete()?;

        let mut text = self.out.join("\n");
        text.push('\n');
        Ok(text)
    }

    /// Collects every @macro ... @end definition out of the source into
    /// the macro table
    ///
    /// This will then be used for macro expansion later during the "recursive" phase
    fn collect(&mut self) -> Result<(), LoweringError> {
        let mut lines = self.source.lines().enumerate();
        let mut collected_out_buf = vec![];

        while let Some((line_number, line)) = lines.next() {
            let line_number = line_number + 1;

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(line).trim();

            if line.is_empty() {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();

            match tokens.as_slice() {
                // @file declaration
                ["@file", file_idx, path @ ..] => {
                    // Parse the file index for the file table
                    let file_index = parse_u64(line_number, file_idx)?;

                    // Path in the mapping for later
                    self.filenames.insert(file_index, path.join(" "));
                    collected_out_buf.push(line.to_string());
                }

                // @macro <name> (<param>, <param>, ...)
                ["@macro", name, params @ ..] => {
                    // Parse parameters
                    // Must start with "("
                    let string_params = params.join(" ");
                    if !string_params.starts_with("(") || !string_params.ends_with(")") {
                        return Err(LoweringErrorKind::InvalidArgument {
                            expected: "@macro <name> (<param>, <param>, ...)".to_string(),
                            got: tokens.join(" "),
                        }
                        .with_line(line_number));
                    }

                    let mut param_names = string_params
                        .strip_prefix("(")
                        .unwrap_or(&string_params)
                        .strip_suffix(")")
                        .unwrap_or(&string_params)
                        .split(",")
                        .map(|split_str| split_str.trim().to_string())
                        .collect::<Vec<String>>();

                    // `()` should be no-params, similar to functions we clear param names
                    if param_names.len() == 1 && param_names[0].is_empty() {
                        param_names.clear();
                    }

                    // Collect the body lines until the matching @end
                    let mut body = Vec::new();
                    let mut terminated = false;

                    for (body_number, body_line) in lines.by_ref() {
                        // Strip the comment from this line (we don't want to expand out comments)
                        let body_line = strip_comment(body_line).trim();

                        // We found an @end!
                        if body_line == "@end" {
                            terminated = true;
                            break;
                        }

                        // A Nested macro definitions aren't allowed!
                        if body_line.starts_with("@macro") {
                            return Err(
                                LoweringErrorKind::NestedMacroDefinition.with_line(body_number + 1)
                            );
                        }

                        // Instructions
                        if !body_line.is_empty() {
                            body.push(body_line.to_string());
                        }
                    }

                    // Hit EOF, check if we found an @end or not
                    if !terminated {
                        return Err(LoweringErrorKind::UnterminatedMacro {
                            name: name.to_string(),
                        }
                        .with_line(line_number));
                    }

                    // Find what the body defines itself, for renaming
                    let introduced = collect_introduced(&body, &param_names);

                    let macro_def = Macro {
                        params: param_names,
                        body,
                        introduced,
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
                    collected_out_buf.push(other.join(" "));
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
        let mut lines = current_expansion_source.into_iter().enumerate();

        // The destination of all expanded things from this pass
        let mut current_expansion_out = vec![];

        // Whether or not a macro invocation was found this pass
        let mut found_macro_invocation = false;

        while let Some((line_number, line)) = lines.next() {
            let line_number = line_number + 1;

            // Strip the comment from a line and ignore if empty, this means
            // we only parse actual tokens
            let line = strip_comment(&line).trim();

            if line.is_empty() {
                continue;
            }

            let tokens: Vec<&str> = line.split_whitespace().collect();

            match tokens.as_slice() {
                // !<name> <file_idx> <original_line> <arg> { <arg> } ...
                [invocation, file_indx, original_line, rest @ ..]
                    if invocation.starts_with("!") =>
                {
                    // Grab the name of the macro we are
                    let name = invocation.strip_prefix("!").unwrap_or(*invocation);
                    found_macro_invocation = true;

                    let Some(macro_def) = self.macros.get(name) else {
                        return Err(LoweringErrorKind::UndefinedMacro {
                            name: name.to_string(),
                        }
                        .with_line(line_number));
                    };

                    // Grab the file index and get the actual name
                    let file_index = parse_u64(line_number, file_indx)?;
                    let original_line = parse_u64(line_number, original_line)?;
                    let file_name = self.filenames.get(&file_index).ok_or_else(|| {
                        LoweringErrorKind::FileIndexNotDefined { index: file_index }
                            .with_line(line_number)
                    })?;

                    // Make sure we are under the expansion limit to prevent infinitely-recursive expansions.
                    self.expansions += 1;

                    if self.expansions > self.max_expansions {
                        return Err(LoweringErrorKind::ExpansionLimit {
                            name: name.to_string(),
                        }
                        .with_line(line_number));
                    }

                    // Parse all the arguments to this macro invocation
                    let rest = rest.iter().map(|token| token.to_string()).collect();
                    let args =
                        parse_args(&mut lines, line_number, rest, macro_def.params.len(), name)?;

                    // Expand the actual macro here
                    let expanded = expand_macro_out_into_lines(
                        file_name,
                        macro_def,
                        &args,
                        self.expansions,
                        name,
                        line_number,
                        original_line,
                    )?;

                    // Push the expanded lines back into the output
                    current_expansion_out.extend(expanded);
                }

                // Everything else passes through untouched.
                other => current_expansion_out.push(other.join(" ")),
            }
        }

        // Update the output from this pass
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

/// Find every name a macro body defines that should be
/// remapped for hygiene purposes
fn collect_introduced(body: &[String], params: &[String]) -> Vec<String> {
    let mut introduced = Vec::new();

    for line in body {
        let tokens: Vec<&str> = line.split_whitespace().collect();

        let name = match tokens.as_slice() {
            // @local <name> defines a local that should be remapped
            ["@local", name] => name.to_string(),

            // <name>: defines a label that should be remapped
            [first, ..] if first.ends_with(":") => {
                first.strip_suffix(":").unwrap_or(*first).to_string()
            }

            _ => continue,
        };

        if !params.contains(&name) && !introduced.contains(&name) {
            introduced.push(name);
        }
    }

    introduced
}

/// Parses all the arguments to a macro invocation starting from a certain line
/// in the set of `lines`.
///
/// `current` should be the remaining in-line elements to the macro invocation.
///
/// `arg_count` is the total number of arguments to the macro which is attempted to be matched.
fn parse_args(
    lines: &mut Enumerate<IntoIter<String>>,
    line_number: usize,
    mut current: Vec<String>,
    arg_count: usize,
    name: &str,
) -> Result<Vec<MacroArg>, LoweringError> {
    // The output args
    let mut args = Vec::with_capacity(arg_count);

    // Parse all of our required args
    while args.len() < arg_count {
        // Out of tokens on this line, continue on the next one.
        if current.is_empty() {
            let Some((_, line)) = lines.next() else {
                return Err(LoweringErrorKind::MissingMacroArguments {
                    name: name.to_string(),
                    expected: arg_count as u64,
                    got: args.len() as u64,
                }
                .with_line(line_number));
            };

            // We still ignore comments during expansion as they shouldn't count as actual lines
            current = strip_comment(&line)
                .split_whitespace()
                .map(|token| token.to_string())
                .collect();

            // And ignore @loc lines to prevent accidentally eating them as input from the linker
            if current.first().is_some_and(|token| token == "@loc") {
                current.clear();
            }

            continue;
        }

        // The first token, we match based on this if it a block or something
        let token = current.remove(0);

        match token.as_str() {
            // A block argument of lines
            "{" => args.push(MacroArg::Block(parse_block(
                lines,
                &mut current,
                line_number,
                name,
            )?)),

            // A closing brace outside of any block is invalid
            // the same as @end
            "}" => {
                return Err(LoweringErrorKind::InvalidArgument {
                    expected: format!("an argument to !{name}"),
                    got: "}".to_string(),
                }
                .with_line(line_number));
            }

            // A plain token argument
            _ => args.push(MacroArg::Token(token)),
        }
    }

    // Leftover tokens after the final argument on the same line are not permitted
    if !current.is_empty() {
        return Err(LoweringErrorKind::MacroInvocationLeftoverTokens {
            name: name.to_string(),
        }
        .with_line(line_number));
    }

    Ok(args)
}

/// Collects the lines of a `{ }` block argument to a macro invocation
///
/// Blocks may span many lines and contain nested braces, line structure
/// is preserved.
fn parse_block(
    lines: &mut Enumerate<IntoIter<String>>,
    current: &mut Vec<String>,
    line_number: usize,
    name: &str,
) -> Result<Vec<String>, LoweringError> {
    // The full block
    let mut block = Vec::new();
    let mut depth = 1u64;

    // Tokens gathered for the block line currently being built.
    let mut building: Vec<String> = Vec::new();

    loop {
        // The current line in this macro block no longer contains any tokens,
        // continue onto the next
        if current.is_empty() {
            // Add the current line onto the full block
            if !building.is_empty() {
                block.push(building.join(" "));
                building.clear();
            }

            let Some((_, line)) = lines.next() else {
                return Err(LoweringErrorKind::UnterminatedBlock {
                    name: name.to_string(),
                }
                .with_line(line_number));
            };

            *current = strip_comment(&line)
                .split_whitespace()
                .map(|token| token.to_string())
                .collect();

            continue;
        }

        // Once again, we need to check the starting token to
        // see if its part of our block, or a sub-block in which
        // the depth increases
        let token = current.remove(0);

        match token.as_str() {
            // Entering a new block
            "{" => {
                depth += 1;
                building.push(token);
            }

            // Closing a block
            "}" => {
                depth -= 1;

                // The original block we are trying to
                // parse is now closed!
                if depth == 0 {
                    if !building.is_empty() {
                        block.push(building.join(" "));
                    }

                    return Ok(block);
                }

                // Else we've just got a nested block
                // which we leave for recursive expansions
                building.push(token);
            }

            _ => building.push(token),
        }
    }
}

/// Produce one expansion of a macro's output block
///
/// This essentially just takes the macro def, all arguments
/// and the unique ID of this macro for hygiene reasons./
fn expand_macro_out_into_lines(
    file_name: &String,
    macro_def: &Macro,
    args: &[MacroArg],
    id: u64,
    macro_name: &str,
    line_number: usize,
    loc_match_line: u64,
) -> Result<Vec<String>, LoweringError> {
    // The output lines of this macro expansion
    let mut out = Vec::new();

    // We need to now expand each line in the macro def
    for body_line in &macro_def.body {
        let tokens: Vec<&str> = body_line.split_whitespace().collect();

        // A parameter alone on a line essentially expands the parameter out at that line
        if let [lone_param] = tokens.as_slice() {
            // Map the param to the arg and insert at this position
            if let Some(index) = macro_def
                .params
                .iter()
                .position(|param| param == lone_param)
            {
                match &args[index] {
                    MacroArg::Token(token) => out.push(token.clone()),
                    MacroArg::Block(lines) => out.extend(lines.iter().cloned()),
                }
                continue;
            }
        }

        // Update macro context with invocation info
        if let ["@loc", rest @ ..] = tokens.as_slice() {
            out.push(format!(
                "@loc {} macro {} invoked in {} at line {}",
                rest.join(" "),
                macro_name,
                file_name,
                loc_match_line
            ));
            continue;
        }

        // Otherwise the line is rebuilt token by token since params maybe in-line
        let mut rebuilt = Vec::with_capacity(tokens.len());

        for token in tokens {
            // A label definition matches on its name as the parameter,
            // we keep the `:`
            let colon = token.ends_with(":");
            let stem = if colon {
                token.strip_suffix(":").unwrap_or(token)
            } else {
                token
            };

            // Check if the token is one of our arguments to our macro
            // because in this case we don't want to do any hygienic remapping
            if let Some(index) = macro_def.params.iter().position(|param| param == stem) {
                // For an inline token expansion, we only permit the usage of `Token`-type parameters
                // (blocks don't really make sense lol since they're multiple tokens)
                let MacroArg::Token(argument) = &args[index] else {
                    return Err(LoweringErrorKind::BlockArgumentInline {
                        name: macro_name.to_string(),
                        param: stem.to_string(),
                    }
                    .with_line(line_number));
                };

                // Rebuild the label/argument
                rebuilt.push(if colon {
                    format!("{argument}:")
                } else {
                    argument.clone()
                });

            // Else if the macro introduced this token uniquely and we should hygienically remap it, then do so.
            } else if macro_def.introduced.iter().any(|intro| intro == stem) {
                rebuilt.push(if colon {
                    format!("{stem}#macro_{id}:")
                } else {
                    format!("{stem}#macro_{id}")
                });
            } else {
                rebuilt.push(token.to_string());
            }
        }

        out.push(rebuilt.join(" "));
    }

    Ok(out)
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

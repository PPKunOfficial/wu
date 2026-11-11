use base64::Engine as _;
use gpui::{Hsla, Rgba};
use ratex_layout::{LayoutOptions, layout, to_display_list, validate_layout};
use ratex_parser::parser::parse;
use ratex_render::{RenderOptions, render_to_png};
use ratex_types::color::Color as MathColor;
use ratex_types::math_style::MathStyle;

const MAX_DISPLAY_FORMULAS: usize = 64;
const MATH_FONT_SIZE: f32 = 16.;

/// Replaces display math (`$$...$$`) with rendered images, and converts inline
/// math (`$...$`) to unicode text. Inline formulas must stay part of the text
/// flow: markdown renders images as separate flex items, which breaks line
/// wrapping and baseline alignment inside a sentence.
pub(crate) fn preprocess_math(markdown: &str, text_color: Hsla) -> String {
    let Rgba { r, g, b, a } = text_color.to_rgb();
    let color = MathColor::new(r, g, b, a);
    let mut output = String::with_capacity(markdown.len());
    let mut index = 0;
    let mut display_count = 0;
    let mut in_code_span = false;

    while index < markdown.len() {
        let rest = &markdown[index..];
        let Some(character) = rest.chars().next() else {
            break;
        };

        if character == '`' && !is_escaped(markdown, index) {
            in_code_span = !in_code_span;
            output.push(character);
            index += character.len_utf8();
            continue;
        }

        if character != '$' || in_code_span || is_escaped(markdown, index) {
            output.push(character);
            index += character.len_utf8();
            continue;
        }

        let display = rest.starts_with("$$");
        let delimiter = if display { "$$" } else { "$" };
        let content_start = index + delimiter.len();
        let Some(relative_end) = find_closing(markdown, content_start, display) else {
            output.push_str(delimiter);
            index = content_start;
            continue;
        };
        let content_end = content_start + relative_end;
        let formula = markdown[content_start..content_end].trim();

        if formula.is_empty() || formula.contains('\n') {
            output.push_str(&markdown[index..content_end + delimiter.len()]);
            index = content_end + delimiter.len();
            continue;
        }

        if display {
            if display_count < MAX_DISPLAY_FORMULAS
                && let Ok(image) = render_formula_png(formula, color)
            {
                display_count += 1;
                output.push_str("\n\n![](data:image/png;base64,");
                output.push_str(&base64::engine::general_purpose::STANDARD.encode(image));
                output.push_str(")\n\n");
                index = content_end + delimiter.len();
                continue;
            }
            output.push_str(&markdown[index..content_end + delimiter.len()]);
            index = content_end + delimiter.len();
            continue;
        }

        output.push_str(&latex_to_unicode(formula));
        index = content_end + delimiter.len();
    }

    output
}

fn find_closing(source: &str, start: usize, display: bool) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'$' => {
                if display {
                    if bytes.get(index + 1) == Some(&b'$') {
                        return Some(index - start);
                    }
                    index += 2;
                } else {
                    return Some(index - start);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn is_escaped(source: &str, index: usize) -> bool {
    let mut backslashes = 0;
    for byte in source[..index].bytes().rev() {
        if byte == b'\\' {
            backslashes += 1;
        } else {
            break;
        }
    }
    backslashes % 2 == 1
}

fn render_formula_png(latex: &str, color: MathColor) -> Result<Vec<u8>, String> {
    let ast = parse(latex).map_err(|error| format!("failed to parse formula: {error}"))?;
    let layout_options = LayoutOptions::default()
        .with_style(MathStyle::Display)
        .with_color(color);
    let layout_box = layout(&ast, &layout_options);
    validate_layout(&layout_box)?;
    let display_list = to_display_list(&layout_box);
    render_to_png(
        &display_list,
        &RenderOptions {
            font_size: MATH_FONT_SIZE,
            padding: 1.,
            background_color: MathColor::new(0., 0., 0., 0.),
            font_dir: String::new(),
            device_pixel_ratio: 1.,
        },
    )
}

fn latex_to_unicode(latex: &str) -> String {
    let chars: Vec<char> = latex.chars().collect();
    let mut output = String::new();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        match character {
            '\\' => {
                index += 1;
                let (command, next) = read_command(&chars, index);
                index = next;
                output.push_str(&apply_command(&command, &chars, &mut index));
            }
            '^' => {
                index += 1;
                let (content, next) = read_group(&chars, index);
                index = next;
                output.push_str(&to_superscript(&content));
            }
            '_' => {
                index += 1;
                let (content, next) = read_group(&chars, index);
                index = next;
                output.push_str(&to_subscript(&content));
            }
            '{' | '}' => index += 1,
            '&' => {
                output.push(' ');
                index += 1;
            }
            _ => {
                output.push(character);
                index += 1;
            }
        }
    }
    output
}

fn read_command(chars: &[char], start: usize) -> (String, usize) {
    let mut index = start;
    while index < chars.len() && chars[index].is_ascii_alphabetic() {
        index += 1;
    }
    if index == start {
        return (
            chars.get(start).map(|c| c.to_string()).unwrap_or_default(),
            index + 1,
        );
    }
    (chars[start..index].iter().collect(), index)
}

fn read_group(chars: &[char], start: usize) -> (String, usize) {
    let mut index = start;
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }
    if chars.get(index) != Some(&'{') {
        let character = chars.get(index).copied().unwrap_or_default();
        return (character.to_string(), (index + 1).min(chars.len()));
    }
    let mut depth = 0usize;
    let mut content = String::new();
    while index < chars.len() {
        match chars[index] {
            '{' => {
                depth += 1;
                if depth > 1 {
                    content.push('{');
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    index += 1;
                    break;
                }
                content.push('}');
            }
            other => content.push(other),
        }
        index += 1;
    }
    (content, index)
}

fn apply_command(command: &str, chars: &[char], index: &mut usize) -> String {
    match command {
        "frac" | "dfrac" | "tfrac" => {
            let (numerator, next) = read_group(chars, *index);
            let (denominator, next) = read_group(chars, next);
            *index = next;
            let numerator = latex_to_unicode(&numerator);
            let denominator = latex_to_unicode(&denominator);
            if needs_parentheses(&numerator) || needs_parentheses(&denominator) {
                format!("({numerator})/({denominator})")
            } else {
                format!("{numerator}/{denominator}")
            }
        }
        "sqrt" => {
            let mut next = *index;
            let root = if chars.get(next) == Some(&'[') {
                let (root, after) = read_group(chars, next + 1);
                let close = chars[after..].iter().position(|c| *c == ']');
                next = close.map(|offset| after + offset + 1).unwrap_or(after);
                Some(root)
            } else {
                None
            };
            let (content, after) = read_group(chars, next);
            *index = after;
            let content = latex_to_unicode(&content);
            match root.as_deref() {
                Some("3") => format!("∛({content})"),
                Some("4") => format!("∜({content})"),
                Some(_) | None => format!("√({content})"),
            }
        }
        "text" | "textrm" | "mathrm" | "mathbf" | "mathit" | "mathcal" | "mathbb" | "mathsf"
        | "mathtt" | "operatorname" | "mbox" => {
            let (content, next) = read_group(chars, *index);
            *index = next;
            content
        }
        "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "displaystyle" | "textstyle"
        | "scriptstyle" | "limits" | "nolimits" => String::new(),
        "quad" | "qquad" | "enspace" => " ".to_string(),
        "," | ";" | ":" | "!" => String::new(),
        " " => " ".to_string(),
        "{" => "{".to_string(),
        "}" => "}".to_string(),
        "$" => "$".to_string(),
        "%" => "%".to_string(),
        "&" => "&".to_string(),
        "#" => "#".to_string(),
        "_" => "_".to_string(),
        "\\" => "\n".to_string(),
        "binom" | "dbinom" | "tbinom" => {
            let (upper, next) = read_group(chars, *index);
            let (lower, next) = read_group(chars, next);
            *index = next;
            format!(
                "C({},{})",
                latex_to_unicode(&upper),
                latex_to_unicode(&lower)
            )
        }
        other => symbol_for(other)
            .map(str::to_string)
            .unwrap_or_else(|| latex_to_unicode(other)),
    }
}

fn needs_parentheses(value: &str) -> bool {
    value.chars().any(|c| "+-*/=<>".contains(c))
}

fn symbol_for(command: &str) -> Option<&'static str> {
    Some(match command {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ε",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "rho" => "ρ",
        "sigma" => "σ",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "φ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "propto" => "∝",
        "times" => "×",
        "cdot" => "·",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "infty" => "∞",
        "partial" => "∂",
        "nabla" => "∇",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "leftrightarrow" => "↔",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "supset" => "⊃",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "setminus" => "∖",
        "emptyset" => "∅",
        "forall" => "∀",
        "exists" => "∃",
        "neg" => "¬",
        "land" => "∧",
        "lor" => "∨",
        "sum" => "∑",
        "prod" => "∏",
        "int" => "∫",
        "oint" => "∮",
        "ldots" | "dots" => "…",
        "cdots" => "⋯",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "log" | "ln" | "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "arcsin" | "arccos"
        | "arctan" | "exp" | "max" | "min" | "gcd" | "lcm" | "lim" | "bmod" | "pmod" => "",
        "mod" => " mod ",
        _ => return None,
    })
}

fn to_superscript(source: &str) -> String {
    if source.chars().all(|c| superscript_for(c).is_some()) && !source.is_empty() {
        source
            .chars()
            .filter_map(superscript_for)
            .collect::<String>()
    } else {
        format!("^({source})")
    }
}

fn to_subscript(source: &str) -> String {
    if source.chars().all(|c| subscript_for(c).is_some()) && !source.is_empty() {
        source.chars().filter_map(subscript_for).collect::<String>()
    } else {
        format!("_({source})")
    }
}

fn superscript_for(character: char) -> Option<char> {
    Some(match character {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'n' => 'ⁿ',
        'i' => 'ⁱ',
        'k' => 'ᵏ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'j' => 'ʲ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'z' => 'ᶻ',
        _ => return None,
    })
}

fn subscript_for(character: char) -> Option<char> {
    Some(match character {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::preprocess_math;
    use gpui::black;

    #[test]
    fn converts_inline_formulas_to_unicode_text() {
        let markdown = "输入两个整数 $a,b$，输出它们的和。";
        let result = preprocess_math(markdown, black());
        assert_eq!(result, "输入两个整数 a,b，输出它们的和。");
        assert!(!result.contains("data:image/png;base64,"));
    }

    #[test]
    fn converts_common_commands_and_exponents() {
        let markdown = "对于所有数据，$-10^9 \\le a,b \\le 10^9$。";
        let result = preprocess_math(markdown, black());
        assert_eq!(result, "对于所有数据，-10⁹ ≤ a,b ≤ 10⁹。");
    }

    #[test]
    fn renders_display_formulas_as_images() {
        let markdown = "$$\\frac{a}{b}$$";
        let result = preprocess_math(markdown, black());
        assert!(result.contains("data:image/png;base64,"));
        assert!(!result.contains("$$"));
    }

    #[test]
    fn keeps_unparseable_display_formulas_as_text() {
        let markdown = "bad $$\\frac{1}{$$ formula";
        let result = preprocess_math(markdown, black());
        assert!(result.contains("$$\\frac{1}{$$"));
    }

    #[test]
    fn does_not_touch_formulas_inside_code_spans() {
        let markdown = "use `$a$` literally";
        let result = preprocess_math(markdown, black());
        assert_eq!(result, markdown);
    }
}

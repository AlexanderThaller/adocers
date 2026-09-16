//! `AsciiMath`, converted to Typst's own mathematics.
//!
//! Typst's mathematics syntax was built along `AsciiMath` lines, and the two
//! look close enough that one is tempted to hand an equation straight over.
//! That does not work: `int_0^1` is an integral in `AsciiMath` and the name
//! Typst already uses for the whole-number type, `xx` is a multiplication sign
//! in one and two variables in the other, and `/` makes a fraction out of
//! whatever happens to be either side of it.
//!
//! So the equation is parsed rather than copied. `asciimath-rs` — already here
//! for the `MathML` a page carries — hands back a tree, and this walks it,
//! writing each node as what Typst calls the same thing. What a node has no
//! Typst name for is written as nothing rather than as a guess.

use asciimath_rs::{
    elements::{
        Element,
        accent::ExpressionAccent,
        group::Group,
        literal::Literal,
        special::{
            Expression,
            Special,
        },
    },
    tokens::{
        Accent,
        Arrow,
        FontCommand,
        Function,
        Greek,
        Logical,
        Misc,
        Operation,
        Relation,
    },
};

/// Translate one `AsciiMath` equation to Typst mathematics.
///
/// `None` for an equation that comes back empty, which is what a parse that
/// found nothing it understood looks like.
pub(crate) fn typst(source: &str) -> Option<String> {
    let source = source.trim();

    if source.is_empty() {
        return None;
    }

    let converted = expression(&asciimath_rs::parse(source));

    (!converted.trim().is_empty()).then_some(converted)
}

/// A sequence of elements, with the spaces Typst needs to tell them apart.
///
/// A second script on one base — the `^j` of `a_i^j` — reaches here as a bare
/// `^` and its argument beside what they belong to, rather than built into it,
/// so they are joined back on.
fn expression(expression: &Expression) -> String {
    let mut out: Vec<String> = Vec::with_capacity(expression.children.len());
    let mut children = expression.children.iter().peekable();

    while let Some(child) = children.next() {
        if let Some(position) = script_marker(child) {
            match (out.pop(), children.next()) {
                (Some(base), Some(script)) => {
                    out.push(format!("attach({base}, {position}: {})", bare(script)));

                    continue;
                }

                // Nothing to attach it to, or nothing to attach: put back what
                // was taken and carry on.
                (base, script) => {
                    out.extend(base);
                    out.extend(script.map(element));

                    continue;
                }
            }
        }

        let written = element(child);

        if !written.is_empty() {
            out.push(written);
        }
    }

    out.join(" ")
}

/// Whether an element is a bare `^` or `_` waiting for what follows it.
fn script_marker(element: &Element) -> Option<&'static str> {
    match element {
        Element::Literal(Literal::Misc(Misc::Pow)) => Some("t"),
        Element::Literal(Literal::Misc(Misc::Sub)) => Some("b"),
        _ => None,
    }
}

/// One node of the tree.
fn element(element: &Element) -> String {
    match element {
        Element::Literal(literal) => self::literal(literal),
        Element::Special(special) => self::special(special),
        Element::Group(group) => self::group(group),
        Element::Accent(accent) => self::accent(accent),
        Element::Null => String::new(),
    }
}

/// An element after a `^` or a `_`, where a pair of brackets is Typst's way of
/// saying "all of this" and is not drawn.
fn script(element: &Element) -> String {
    format!("({})", bare(element))
}

/// An element with the brackets it was written in taken off.
fn bare(element: &Element) -> String {
    self::element(&element.to_non_enclosed())
}

/// The structures that carry other elements: sums, fractions, powers.
fn special(special: &Special) -> String {
    match special {
        Special::Sum(sum) => limited("sum", sum.top.as_deref(), sum.bottom.as_deref()),
        Special::Prod(prod) => limited("product", prod.top.as_deref(), prod.bottom.as_deref()),

        Special::Integral(integral) => limited(
            "integral",
            integral.top.as_deref(),
            integral.bottom.as_deref(),
        ),

        Special::OIntegral(integral) => limited(
            "integral.cont",
            integral.top.as_deref(),
            integral.bottom.as_deref(),
        ),

        Special::Frac(frac) => format!("frac({}, {})", element(&frac.top), element(&frac.bottom)),

        // Attached rather than written with `^` and `_`: those bind to the one
        // token before them, and a base wrapped in brackets to hold it together
        // would have those brackets drawn — Typst renders a pair of them in
        // mathematics rather than reading them as grouping.
        Special::Pow(pow) => format!("attach({}, t: {})", bare(&pow.base), bare(&pow.exp)),
        Special::Sub(sub) => format!("attach({}, b: {})", bare(&sub.base), bare(&sub.lower)),
        // The brackets around a root's contents are how it was written rather
        // than part of what it says, so `sqrt(4)` is a root over 4 and not a
        // root over a bracketed 4.
        Special::Sqrt(sqrt) => format!("sqrt({})", bare(&sqrt.inner)),

        Special::Root(root) => format!("root({}, {})", bare(&root.base), bare(&root.inner)),
    }
}

/// An operator with whatever limits it was given.
fn limited(operator: &str, top: Option<&Element>, bottom: Option<&Element>) -> String {
    let mut out = operator.to_string();

    if let Some(bottom) = bottom {
        out.push('_');
        out.push_str(&script(bottom));
    }

    if let Some(top) = top {
        out.push('^');
        out.push_str(&script(top));
    }

    out
}

/// The bracketed forms, and the two kinds of table.
fn group(group: &Group) -> String {
    match group {
        Group::MSep => ",".to_string(),
        Group::Parentheses(inner) => format!("lr(( {} ))", expression(&inner.inner)),
        Group::Brackets(inner) => format!("lr([ {} ])", expression(&inner.inner)),
        Group::Braces(inner) => format!("lr({{ {} }})", expression(&inner.inner)),

        Group::Angles(inner) => format!("lr(chevron.l {} chevron.r)", expression(&inner.inner)),
        Group::XGroup(inner) => format!("lr(chevron.l {} chevron.r)", expression(&inner.inner)),

        Group::Abs(inner) => format!("abs({})", expression(&inner.inner)),
        Group::Floor(inner) => format!("floor({})", expression(&inner.inner)),
        Group::Ceil(inner) => format!("ceil({})", expression(&inner.inner)),
        Group::Norm(inner) => format!("norm({})", expression(&inner.inner)),

        Group::Matrix(matrix) => table("[", &matrix.inner),
        Group::Vector(vector) => table("(", &vector.inner),

        // A group whose brackets were consumed by whatever it is an argument
        // of: the contents stand on their own.
        Group::NonEnclosed(inner) => expression(&inner.inner),
    }
}

/// A matrix or vector, as the rows and columns Typst writes them in.
fn table(delimiter: &str, rows: &[Vec<Expression>]) -> String {
    let rows: Vec<String> = rows
        .iter()
        .map(|row| row.iter().map(expression).collect::<Vec<_>>().join(", "))
        .collect();

    format!("mat(delim: \"{delimiter}\", {})", rows.join("; "))
}

/// A mark set over or under something.
fn accent(accent: &ExpressionAccent) -> String {
    match accent {
        ExpressionAccent::Generic(generic) => {
            let inner = element(&generic.inner);

            match generic.accent {
                Accent::Hat => format!("hat({inner})"),
                Accent::Overline => format!("overline({inner})"),
                Accent::Underline => format!("underline({inner})"),
                Accent::Vec => format!("arrow({inner})"),
                Accent::Dot => format!("dot({inner})"),
                Accent::DDot => format!("dot.double({inner})"),
                Accent::OverBrace => format!("overbrace({inner})"),
                Accent::UnderBrace => format!("underbrace({inner})"),
                Accent::Cancel => format!("cancel({inner})"),

                // The mark is the structure rather than a mark of its own, and
                // is handled by the arms below.
                Accent::OverSet | Accent::UnderSet | Accent::Color(_) => inner,
            }
        }

        // Which of the two is the base is not what the field names suggest:
        // these follow the reference rendering rather than the names.
        ExpressionAccent::OverSet(set) => {
            format!("limits({})^({})", element(&set.bottom), element(&set.top))
        }

        ExpressionAccent::UnderSet(set) => {
            format!("limits({})_({})", element(&set.top), element(&set.bottom))
        }

        // The colour of a printed page is the page's to choose, so the content
        // is kept and the colour dropped.
        ExpressionAccent::Color(colour) => element(&colour.inner),
    }
}

/// A leaf: a number, a name, a symbol, an operator.
fn literal(literal: &Literal) -> String {
    match literal {
        Literal::Number(number) => number.number.clone(),
        Literal::Symbol(symbol) => name(&symbol.symbol),
        Literal::Greek(greek) => self::greek(greek).to_string(),
        Literal::Relation(relation) => self::relation(relation).to_string(),
        Literal::Logical(logical) => self::logical(logical).to_string(),
        Literal::Arrow(arrow) => self::arrow(arrow).to_string(),
        Literal::Misc(misc) => self::misc(misc).to_string(),
        Literal::Operation(operation) => self::operation(operation).to_string(),
        Literal::Function(function) => self::function(function),
        Literal::NewLine => "\\".to_string(),

        // Words, which Typst sets upright when they are quoted.
        Literal::Text(text) => {
            let quoted = format!(
                "\"{}\"",
                text.text.replace('\\', "\\\\").replace('"', "\\\"")
            );

            match &text.formatting {
                Some(font) => format!("{}({quoted})", self::font(font)),
                None => quoted,
            }
        }

        // A font command with nothing to apply to says nothing by itself.
        Literal::FontCommand(_) => String::new(),
    }
}

/// One identifier or symbol, written so Typst reads it as itself.
///
/// A name of more than one letter is a variable in Typst and a run of separate
/// letters in `AsciiMath`, so it is spaced out; anything that is not a letter
/// or a digit is escaped, since the punctuation of one language is the syntax
/// of the other.
fn name(symbol: &str) -> String {
    if symbol.chars().all(char::is_alphabetic) && symbol.chars().count() > 1 {
        return symbol
            .chars()
            .map(|letter| letter.to_string())
            .collect::<Vec<_>>()
            .join(" ");
    }

    symbol
        .chars()
        .map(|character| match character {
            '*' | '_' | '^' | '$' | '#' | '&' | '/' | '\\' | '"' | '<' | '>' | '@' => {
                format!("\\{character}")
            }

            _ => character.to_string(),
        })
        .collect()
}

/// The face a `bb`, `bbb`, `cc`, `tt`, `fr` or `sf` asks for.
fn font(font: &FontCommand) -> &'static str {
    match font {
        FontCommand::Big => "bold",
        FontCommand::BigOutline => "bb",
        FontCommand::Cursive => "cal",
        FontCommand::TText => "mono",
        FontCommand::Fr => "frak",
        FontCommand::SansSerif => "sans",
    }
}

/// The Greek letters, under the names Typst gives them.
fn greek(greek: &Greek) -> &'static str {
    match greek {
        Greek::Alpha => "alpha",
        Greek::Beta => "beta",
        Greek::Gamma => "gamma",
        Greek::BigGamma => "Gamma",
        Greek::Delta => "delta",
        Greek::BigDelta => "Delta",
        Greek::Epsilon => "epsilon",
        Greek::VarEpsilon => "epsilon.alt",
        Greek::Zeta => "zeta",
        Greek::Eta => "eta",
        Greek::Theta => "theta",
        Greek::BigTheta => "Theta",
        Greek::VarTheta => "theta.alt",
        Greek::Iota => "iota",
        Greek::Kappa => "kappa",
        Greek::Lambda => "lambda",
        Greek::BigLambda => "Lambda",
        Greek::Mu => "mu",
        Greek::Nu => "nu",
        Greek::Xi => "xi",
        Greek::BigXi => "Xi",
        Greek::Pi => "pi",
        Greek::BigPi => "Pi",
        Greek::Rho => "rho",
        Greek::Sigma => "sigma",
        Greek::BigSigma => "Sigma",
        Greek::Tau => "tau",
        Greek::Upsilon => "upsilon",
        Greek::Phi => "phi",
        Greek::BigPhi => "Phi",
        Greek::VarPhi => "phi.alt",
        Greek::Chi => "chi",
        Greek::Psi => "psi",
        Greek::BigPsi => "Psi",
        Greek::Omega => "omega",
        Greek::BigOmega => "Omega",
    }
}

/// The relations: what stands between two sides of a statement.
fn relation(relation: &Relation) -> &'static str {
    match relation {
        Relation::Eq => "=",
        Relation::Ne => "!=",
        Relation::Lt => "<",
        Relation::Gt => ">",
        Relation::Le => "<=",
        Relation::Ge => ">=",
        Relation::Prec => "prec",
        Relation::PrecEq => "prec.eq",
        Relation::Succ => "succ",
        Relation::SuccEq => "succ.eq",
        Relation::In => "in",
        Relation::NotIn => "in.not",
        Relation::SubSet => "subset",
        Relation::SupSet => "supset",
        Relation::SubSetEq => "subset.eq",
        Relation::SupSetEq => "supset.eq",
        Relation::Equiv => "equiv",
        Relation::Cong => "tilde.equiv",
        Relation::Approx => "approx",
        Relation::PropTo => "prop",
    }
}

/// The logical connectives and quantifiers.
fn logical(logical: &Logical) -> &'static str {
    match logical {
        Logical::And => "and",
        Logical::Or => "or",
        Logical::Not => "not",
        Logical::Implies => "=>",
        Logical::If => "\"if\"",
        Logical::Iff => "<=>",
        Logical::ForAll => "forall",
        Logical::Exists => "exists",
        Logical::Bot => "bot",
        Logical::Top => "top",
        Logical::VDash => "tack.r",
        Logical::Models => "tack.r.double",
    }
}

/// The arrows.
fn arrow(arrow: &Arrow) -> &'static str {
    match arrow {
        Arrow::UpArrow => "arrow.t",
        Arrow::DownArrow => "arrow.b",
        Arrow::RightArrow | Arrow::To => "arrow.r",
        Arrow::RightArrowTail => "arrow.r.tail",
        // Typst draws no two-headed arrow with a tail, so both come out as the
        // two-headed one: the head is what the arrow is about.
        Arrow::TwoHeadRightArrow | Arrow::TwoHeadRightArrowTail => "arrow.r.twohead",
        Arrow::MapsTo => "arrow.r.bar",
        Arrow::LeftArrow => "arrow.l",
        Arrow::LeftRightArrow => "arrow.l.r",
        Arrow::BigRightArrow => "arrow.r.double",
        Arrow::BigLeftArrow => "arrow.l.double",
        Arrow::BigLeftRightArrow => "arrow.l.r.double",
    }
}

/// The symbols that belong to no other group.
fn misc(misc: &Misc) -> &'static str {
    match misc {
        Misc::Int => "integral",
        Misc::OInt => "integral.cont",
        Misc::Del => "partial",
        Misc::Grad => "nabla",
        Misc::PlusMinus => "plus.minus",
        Misc::EmptySet => "nothing",
        Misc::Infty => "infinity",
        Misc::Aleph => "alef",
        Misc::Therefore => "therefore",
        Misc::Because => "because",
        Misc::PLDots => "dots.h",
        Misc::PCDots => "dots.c",
        Misc::VDots => "dots.v",
        Misc::DDots => "dots.down",
        Misc::EPipes => "parallel",
        Misc::EQuad => "quad",
        Misc::Angle => "angle",
        Misc::Frown => "frown",
        Misc::Triangle => "triangle",
        Misc::Diamond => "diamond",
        Misc::Square => "square",
        Misc::LFloor => "floor.l",
        Misc::RFloor => "floor.r",
        Misc::LCeiling => "ceil.l",
        Misc::RCeiling => "ceil.r",
        Misc::Complex => "CC",
        Misc::Natural => "NN",
        Misc::Rational => "QQ",
        Misc::Real => "RR",
        Misc::Integer => "ZZ",

        // The tokens that open a structure rather than standing for something:
        // by the time one reaches here its structure has already been built.
        Misc::AsciiFrac
        | Misc::LatexFrac
        | Misc::Sub
        | Misc::Pow
        | Misc::Sqrt
        | Misc::Root
        | Misc::LatexText => "",
    }
}

/// The operators.
fn operation(operation: &Operation) -> &'static str {
    match operation {
        Operation::Plus => "+",
        Operation::Minus => "-",
        Operation::CDot => "dot.c",
        Operation::Ast => "ast",
        Operation::Star => "star",
        Operation::Slash => "slash",
        Operation::Backslash => "backslash",
        Operation::Times => "times",
        Operation::Div => "div",
        Operation::LTimes => "times.l",
        Operation::RTimes => "times.r",
        Operation::Bowtie => "bowtie",
        Operation::Circ => "compose",
        Operation::OPlus => "plus.o",
        Operation::OTimes => "times.o",
        Operation::ODot => "dot.o",
        Operation::Sum => "sum",
        Operation::Prod => "product",
        Operation::Wedge => "and",
        Operation::BidWedge => "and.big",
        Operation::Vee => "or",
        Operation::BigVee => "or.big",
        Operation::Cap => "inter",
        Operation::BigCap => "inter.big",
        Operation::Cup => "union",
        Operation::BigCup => "union.big",
    }
}

/// The named functions, upright as a function name should be.
fn function(function: &Function) -> String {
    let name = match function {
        Function::Sin => "sin",
        Function::Cos => "cos",
        Function::Tan => "tan",
        Function::Sec => "sec",
        Function::Csc => "csc",
        Function::Cot => "cot",
        Function::ArcSin => "arcsin",
        Function::ArcCos => "arccos",
        Function::ArcTan => "arctan",
        Function::Sinh => "sinh",
        Function::Cosh => "cosh",
        Function::Tanh => "tanh",
        Function::Exp => "exp",
        Function::Log => "log",
        Function::Ln => "ln",
        Function::Det => "det",
        Function::Dim => "dim",
        Function::Mod => "mod",
        Function::Gcd => "gcd",
        Function::Lcm => "lcm",
        Function::Min => "min",
        Function::Max => "max",

        // Names Typst has no operator for, and the two bare letters `f` and `g`
        // that `AsciiMath` counts among its functions.
        Function::Sech => return "op(\"sech\")".to_string(),
        Function::Csch => return "op(\"csch\")".to_string(),
        Function::Coth => return "op(\"coth\")".to_string(),
        Function::Lub => return "op(\"lub\")".to_string(),
        Function::Glb => return "op(\"glb\")".to_string(),
        Function::F => return "f".to_string(),
        Function::G => return "g".to_string(),
    };

    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_arithmetic_a_reader_would_expect() {
        assert_eq!(
            typst("a^2 + b^2 = c^2").as_deref(),
            Some("attach(a, t: 2) + attach(b, t: 2) = attach(c, t: 2)")
        );
    }

    #[test]
    fn names_what_typst_names_differently() {
        let product = typst("2 xx 3 -: 4").expect("converts");

        assert!(
            product.contains("times") && product.contains("div"),
            "{product}"
        );

        let sets = typst("A uu B nn C").expect("converts");

        assert!(sets.contains("union") && sets.contains("inter"), "{sets}");
    }

    #[test]
    fn joins_a_second_script_back_onto_its_base() {
        // The parser hands `^j` over as a mark and an argument standing beside
        // what they belong to rather than built into it.
        assert_eq!(
            typst("a_i^j").as_deref(),
            Some("attach(attach(a, b: i), t: j)")
        );
        assert_eq!(
            typst("x^2_i").as_deref(),
            Some("attach(attach(x, t: 2), b: i)")
        );
    }

    #[test]
    fn takes_the_brackets_off_a_root() {
        // `sqrt(4)` is a root over 4, not a root over a bracketed 4.
        assert_eq!(typst("sqrt(4)").as_deref(), Some("sqrt(4)"));
        assert_eq!(typst("root(3)(8)").as_deref(), Some("root(3, 8)"));
    }

    #[test]
    fn builds_the_structures_rather_than_copying_them() {
        assert!(typst("(x+1)/(x-1)").expect("converts").starts_with("frac("));
        assert!(
            typst("sum_(i=1)^n i")
                .expect("converts")
                .starts_with("sum_(")
        );
        assert!(
            typst("[[a,b],[c,d]]")
                .expect("converts")
                .starts_with("mat(")
        );
    }

    #[test]
    fn spaces_out_a_run_of_letters() {
        // `abc` is three variables multiplied in AsciiMath and one name in
        // Typst, which would set it upright and mean something else.
        assert_eq!(name("abc"), "a b c");
        assert_eq!(name("x"), "x");
    }

    #[test]
    fn declines_an_equation_with_nothing_in_it() {
        assert!(typst("   ").is_none());
    }
}

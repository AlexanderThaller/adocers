// The handlers MiTeX's output calls for, which its own Typst package supplies
// through a scope. Only the conversion is done in Rust here, so the handlers
// have to be defined for the compiler to find.
//
// These are transcribed from MiTeX's `packages/mitex/specs/latex/standard.typ`
// (Apache-2.0), which is the authority on what each generated call means.

// A zero-width space, which stands in an empty cell so a row keeps its shape.
#let zws = math.zws

// `\sqrt{x}` and `\sqrt[n]{x}` come through one name, told apart by how many
// arguments arrive. The index carries its own brackets, which are dropped.
#let mitexsqrt(..args) = {
  if args.pos().len() == 1 {
    math.sqrt(args.pos().at(0))
  } else {
    math.root(
      args.pos().at(0).children.filter(it => it != [\[] and it != [\]]).sum(),
      args.pos().at(1),
    )
  }
}

// The matrix environments, which differ only in what encloses them.
#let matrix = math.mat.with(delim: none)
#let pmatrix = math.mat.with(delim: "(")
#let bmatrix = math.mat.with(delim: "[")
#let Bmatrix = math.mat.with(delim: "{")
#let vmatrix = math.mat.with(delim: "|")
#let Vmatrix = math.mat.with(delim: "||")
#let smallmatrix(..args) = math.inline(math.mat(delim: none, ..args))

// `aligned`, and the environments that come through as it: a displayed block
// whose `&` and `\` are already in the converted body.
#let aligned(..args) = {
  if args.pos().len() == 0 {
    math.zws
  } else {
    pad(y: 0.2em, block(math.op(math.display(args.pos().sum()))))
  }
}

// `alignedat`, which is `aligned` with a column count nothing here needs.
#let alignedat(arg0: none, it) = pad(y: 0.2em, block(math.op(it)))

// The stretchy arrows, which carry their label above them.
#let arrow-with(symbol) = it => $limits(stretch(#symbol))^#it$
#let xleftarrow = arrow-with(math.arrow.l)
#let xrightarrow = arrow-with(math.arrow.r)
#let xLeftarrow = arrow-with(math.arrow.l.double)
#let xRightarrow = arrow-with(math.arrow.r.double)
#let xleftrightarrow = arrow-with(math.arrow.l.r)
#let xLeftrightarrow = arrow-with(math.arrow.l.r.double)
#let xhookleftarrow = arrow-with(math.arrow.l.hook)
#let xhookrightarrow = arrow-with(math.arrow.r.hook)
#let xtwoheadleftarrow = arrow-with(math.arrow.l.twohead)
#let xtwoheadrightarrow = arrow-with(math.arrow.r.twohead)

// The ring accent, which Typst calls by its shape.
#let mathring(it) = math.circle(it)

// `\begin{array}{lcr}`, whose first argument is the column alignment.
#let mitexarray(arg0: ("l",), ..args) = {
  if args.pos().len() == 0 {
    return
  }

  let spec = if type(arg0) == str {
    (arg0,)
  } else if arg0.has("children") {
    arg0
      .children
      .filter(it => it != [ ] and it != [#math.zws])
      .map(it => it.text)
      .filter(it => it == "l" or it == "c" or it == "r")
  } else {
    (arg0.text,)
  }

  let rows = args.pos().map(row => if type(row) == array { row } else { (row,) })
  let columns = calc.max(..rows.map(row => row.len()))
  let cells = rows.map(row => row + (columns - row.len()) * (none,))

  let alignment = ("l": left, "c": center, "r": right)
  set align(alignment.at(spec.at(0), default: left))

  pad(
    y: 0.2em,
    grid(
      columns: columns,
      column-gutter: 0.5em,
      row-gutter: 0.5em,
      ..cells.flatten().map(it => $it$),
    ),
  )
}

// The style and font commands, which take everything after them.
#let mitexdisplay(..args) = math.display(args.pos().sum())
#let mitexinline(..args) = math.inline(args.pos().sum())
#let mitexupright(..args) = math.upright(args.pos().sum())
#let mitexmathbf(it) = math.bold(math.upright(it))

// `\text{…}`: words inside an equation, set as words.
#let textmath(it) = it

// Named operators, and the marks that sit above and below them.
#let operatorname(it) = math.op(math.upright(it))
#let operatornamewithlimits(it) = math.op(limits: true, math.upright(it))
#let stackrel(sup, base) = $limits(base)^(sup)$
#let substack(it) = it
#let boxed(it) = box(stroke: 0.5pt, inset: 6pt, $it$)

// Space where something would have been, and nothing drawn in it.
#let phantom(it) = hide(it)
#let hphantom(it) = box(height: 0pt, hide(it))
#let vphantom(it) = box(width: 0pt, hide(it))

// What a piece of an equation counts as, which decides the space around it.
#let mathbin(it) = math.class("binary", it)
#let mathclose(it) = math.class("closing", it)
#let mathinner(it) = math.class("fence", it)
#let mathop(it) = math.class("unary", it)
#let mathopen(it) = math.class("opening", it)
#let mathord(it) = math.class("normal", it)
#let mathpunct(it) = math.class("punctuation", it)
#let mathrel(it) = math.class("relation", it)

// The braces and brackets that span a run and carry a label.
#let mitexoverbrace(it) = math.limits(math.overbrace(it))
#let mitexunderbrace(it) = math.limits(math.underbrace(it))
#let mitexoverbracket(it) = math.limits(math.overbracket(it))
#let mitexunderbracket(it) = math.limits(math.underbracket(it))

// `\raisebox{2pt}{x}` keeps its content and loses the offset: how far
// something is lifted off the line is presentation, and a lost raise is a much
// smaller loss than a document that will not typeset.
#let raisebox(amount, it) = it

// The delimiters a `\big`, `\Big`, `\bigg` or `\Bigg` asks to be enlarged.
#let big(it) = math.lr(size: 1.2em, it)
#let Big(it) = math.lr(size: 1.8em, it)
#let bigg(it) = math.lr(size: 2.4em, it)
#let Bigg(it) = math.lr(size: 3em, it)

// `\bf`, which takes everything after it, and the struck-through form of
// `\cancel`.
#let mitexbold(..args) = math.bold(math.upright(args.pos().sum()))
#let bcancel = math.cancel.with(inverted: true)

// `\atop`, which stacks one thing on another with nothing between them.
#let atop(a, b) = $mat(delim: #none, #a; #b)$

// Intersection under the name Typst used to call it by, which `\cap` and
// `\bigcap` still come through as — with the modifier `\bigcap` needs, so it
// is the symbol rather than a copy of its character.
#let sect = math.inter

// The two operators that take their subscript below.
#let argmax = math.op("arg max", limits: true)
#let argmin = math.op("arg min", limits: true)

// The operator names Typst does not carry: the Continental spellings of the
// trigonometric functions, which a document written in that convention uses
// and Typst, following the English one, has never heard of.
#let arcctg = math.op("arcctg")
#let arctg = math.op("arctg")
#let ch = math.op("ch")
#let ctg = math.op("ctg")
#let cth = math.op("cth")
#let sh = math.op("sh")
#let tg = math.op("tg")
#let th = math.op("th")

// Typst renamed a good deal of its mathematics between the version `mitex`'s
// tables were written against and the one compiled in here. What follows fills
// that gap: the same meaning, under the name this Typst knows it by. Anything
// not covered falls back to being shown as its source, which is what
// `render::typst::pdf` catches.

// Fractions and binomials in the two sizes LaTeX distinguishes.
#let cfrac(num, den) = $display((num)/(den))$
#let dfrac(num, den) = $display((num)/(den))$
#let tfrac(num, den) = $inline((num)/(den))$
#let dbinom(n, k) = $display(binom(#n, #k))$
#let tbinom(n, k) = $inline(binom(#n, #k))$

// Something set above or below what it belongs to.
#let overset(sup, base) = $limits(base)^(sup)$
#let underset(sub, base) = $limits(base)_(sub)$

// `\text…` in its several weights and faces.
#let textnormal(it) = it
#let textbf(it) = math.bold(it)
#let textit(it) = math.italic(it)
#let textrm(it) = math.upright(it)
#let textmd(it) = it
#let textup(it) = math.upright(it)
#let textsf(it) = math.sans(it)
#let texttt(it) = math.mono(it)

// The faces and sizes that take everything after them.
#let mitexscript(..args) = math.script(args.pos().sum())
#let mitexsscript(..args) = math.sscript(args.pos().sum())
#let mitexitalic(..args) = math.italic(args.pos().sum())
#let mitexsans(..args) = math.sans(args.pos().sum())
#let mitexfrak(..args) = math.frak(args.pos().sum())
#let mitexmono(..args) = math.mono(args.pos().sum())
#let mitexcal(..args) = math.cal(args.pos().sum())

// Modular arithmetic, and a few marks with no Typst name of their own.
#let pmod(it) = $quad (mod thick it)$
#let pod(it) = $quad (it)$
#let mathclap(it) = box(width: 0pt, $it$)
#let underbar(it) = $underline(it)$
#let sout = math.cancel.with(angle: 90deg)
#let diff = math.partial
#let smallint = math.integral

// Space, which LaTeX measures and Typst names.
#let hspace(it) = h(1em)
#let vspace(it) = v(1em)
#let enspace = h(0.5em)
#let negthinspace = h(-(3 / 18) * 1em)
#let negmedspace = h(-(4 / 18) * 1em)
#let negthickspace = h(-(5 / 18) * 1em)
#let negthinmedspace = h(-(4 / 18) * 1em)

// The limits that follow an operator, which arrive with an empty argument.
#let limits(..args) = math.limits(args.pos().sum(default: []))
#let scripts(..args) = math.scripts(args.pos().sum(default: []))

// Colour is the page's to choose here, so a colour command keeps its content
// and drops the colour rather than failing over it. The first argument is the
// colour in every form of these commands; what follows is the content.
#let mitexcolor(..args) = args.pos().slice(1).sum(default: [])
#let colortext(..args) = args.pos().last()
#let mitexcolorbox(..args) = args.pos().last()

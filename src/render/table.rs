//! Table rendering.

use std::fmt::Write as _;

use asciidoc_parser::blocks::{
    Block,
    ColumnStyle,
    Frame,
    Grid,
    HorizontalAlignment,
    IsBlock,
    Stripes,
    TableBlock,
    TableCell,
    TableCellContent,
    TableColumn,
    TableRow,
    VerticalAlignment,
};

use crate::render::{
    Renderer,
    html::escape_attr,
};

impl<'src> Renderer<'src> {
    /// Render a `|===` table.
    pub(super) fn table_block(&mut self, block: &'src Block<'src>, table: &'src TableBlock<'src>) {
        let mut classes = vec!["tableblock".to_string()];
        classes.push(frame_class(table.frame()));
        classes.push(grid_class(table.grid()));

        if let Some(stripes) = stripes_class(table.stripes()) {
            classes.push(stripes);
        }

        // A `[width=NN%]` narrower than the page says how wide the table is
        // outright, which leaves nothing for either sizing class to say: it
        // overrides autowidth as well as the default of filling the page.
        let width = table.width().filter(|&width| width < 100);

        if width.is_none() {
            classes.push(
                if table.is_autowidth() {
                    "fit-content"
                } else {
                    "stretch"
                }
                .to_string(),
            );
        }

        // The block's own roles come last, after every class the table's own
        // shape called for.
        classes.extend(block.roles().into_iter().map(str::to_string));

        let mut open = String::from("<table");

        if let Some(id) = block.id() {
            let _ = write!(open, " id=\"{}\"", escape_attr(id));
        }

        let _ = write!(open, " class=\"{}\"", escape_attr(&classes.join(" ")));

        if let Some(width) = width {
            let _ = write!(open, " style=\"width: {width}%;\"");
        }

        open.push('>');
        self.out.line(&open);

        if let Some(title) = block.title() {
            let caption = block.caption().unwrap_or_default();
            self.out.line(&format!(
                "<caption class=\"title\">{caption}{title}</caption>"
            ));
        }

        self.colgroup(table);

        if let Some(header) = table.header_row() {
            self.out.line("<thead>");
            self.row(header, table.columns(), true);
            self.out.line("</thead>");
        }

        let body = table.body_rows();
        if !body.is_empty() {
            self.out.line("<tbody>");

            for row in body {
                self.row(row, table.columns(), false);
            }

            self.out.line("</tbody>");
        }

        if let Some(footer) = table.footer_row() {
            self.out.line("<tfoot>");
            self.row(footer, table.columns(), false);
            self.out.line("</tfoot>");
        }

        self.out.close("table");
    }

    /// Emit the `<colgroup>` that fixes the relative column widths.
    fn colgroup(&mut self, table: &'src TableBlock<'src>) {
        let columns = table.columns();

        if columns.is_empty() {
            return;
        }

        self.out.line("<colgroup>");

        // With autowidth there is nothing to distribute: the browser measures
        // the content itself.
        let total: usize = columns.iter().map(TableColumn::width).sum();

        // A single `~` column stops the row being a proportion of anything: the
        // measured columns then stand for the percentage they were written as,
        // and the rest is the browser's to divide.
        let mixed = columns.iter().any(TableColumn::is_autowidth);

        let mut shares: Vec<Option<u64>> = columns
            .iter()
            .map(|column| {
                if table.is_autowidth() || total == 0 || column.is_autowidth() {
                    None
                } else if mixed {
                    Some(percentage(column.width(), 100))
                } else {
                    Some(percentage(column.width(), total))
                }
            })
            .collect();

        // Rounding each share on its own leaves the row a hair short of, or
        // over, the full width. The last measured column absorbs the
        // difference, so the widths always add up to exactly 100% — which is
        // only meaningful when every column was measured.
        if !mixed {
            balance(&mut shares);
        }

        for share in shares {
            match share {
                None => self.out.line("<col>"),
                Some(share) => self
                    .out
                    .line(&format!("<col style=\"width: {}%;\">", trim_percent(share))),
            }
        }

        self.out.close("colgroup");
    }

    /// Emit one `<tr>`.
    fn row(&mut self, row: &'src TableRow<'src>, columns: &'src [TableColumn], is_header: bool) {
        self.out.line("<tr>");

        for (index, cell) in row.cells().iter().enumerate() {
            self.cell(cell, columns.get(index), is_header);
        }

        self.out.line("</tr>");
    }

    /// Emit one `<td>` or `<th>`.
    ///
    /// A cell inherits its alignment from its column unless it overrides it,
    /// and a cell whose style is `h` becomes a header cell wherever it appears.
    fn cell(
        &mut self,
        cell: &'src TableCell<'src>,
        column: Option<&'src TableColumn>,
        is_header: bool,
    ) {
        let style = cell.style();
        let header = is_header || style == ColumnStyle::Header;
        let tag = if header { "th" } else { "td" };

        let mut open = format!(
            "<{tag} class=\"tableblock halign-{} valign-{}\"",
            halign(cell.h_align(), column),
            valign(cell.v_align(), column)
        );

        if cell.colspan() > 1 {
            let _ = write!(open, " colspan=\"{}\"", cell.colspan());
        }

        if cell.rowspan() > 1 {
            let _ = write!(open, " rowspan=\"{}\"", cell.rowspan());
        }

        open.push('>');
        self.out.raw(&open);

        self.cell_content(cell, header);

        self.out.line(&format!("</{tag}>"));
    }

    /// Emit the inside of a cell, in whichever of the six cell styles applies.
    fn cell_content(&mut self, cell: &'src TableCell<'src>, header: bool) {
        match cell.content() {
            TableCellContent::Simple(content) => {
                let rendered = content.rendered_html();

                // An empty cell stays empty rather than holding an empty
                // paragraph. A literal one is the exception: its `<pre>` is a
                // visible box, and the row would jump without it.
                if rendered.is_empty() && cell.style() != ColumnStyle::Literal {
                    return;
                }

                // A header cell is already emphasized by its element, so it
                // carries the text directly rather than wrapping it.
                if header {
                    self.out.raw(rendered);
                    return;
                }

                match cell.style() {
                    ColumnStyle::Literal => {
                        self.out.raw(&format!(
                            "<div class=\"literal\"><pre>{rendered}</pre></div>"
                        ));
                    }

                    ColumnStyle::Emphasis => {
                        self.out
                            .raw(&format!("<p class=\"tableblock\"><em>{rendered}</em></p>"));
                    }

                    ColumnStyle::Strong => {
                        self.out.raw(&format!(
                            "<p class=\"tableblock\"><strong>{rendered}</strong></p>"
                        ));
                    }

                    ColumnStyle::Monospace => {
                        self.out.raw(&format!(
                            "<p class=\"tableblock\"><code>{rendered}</code></p>"
                        ));
                    }

                    _ => self
                        .out
                        .raw(&format!("<p class=\"tableblock\">{rendered}</p>")),
                }
            }

            // An AsciiDoc cell is a document in its own right; its blocks
            // render exactly as they would at the top level. The wrapper hugs
            // them, opening on the `<td>`'s line and closing on the last
            // block's, so the cell reads as one run of markup.
            TableCellContent::AsciiDoc(nested) => {
                self.out.raw("<div class=\"content\">");
                self.blocks(nested.blocks().iter());
                self.out.unline();
                self.out.raw("</div>");
            }
        }
    }
}

/// The `frame-*` class for a table.
fn frame_class(frame: Frame) -> String {
    match frame {
        Frame::All => "frame-all",
        Frame::Ends => "frame-ends",
        Frame::Sides => "frame-sides",
        Frame::None => "frame-none",
    }
    .to_string()
}

/// The `grid-*` class for a table.
fn grid_class(grid: Grid) -> String {
    match grid {
        Grid::All => "grid-all",
        Grid::Rows => "grid-rows",
        Grid::Cols => "grid-cols",
        Grid::None => "grid-none",
    }
    .to_string()
}

/// The `stripes-*` class for a table, or `None` when it has no striping.
fn stripes_class(stripes: Stripes) -> Option<String> {
    match stripes {
        Stripes::None => None,
        Stripes::Even => Some("stripes-even".to_string()),
        Stripes::Odd => Some("stripes-odd".to_string()),
        Stripes::All => Some("stripes-all".to_string()),
        Stripes::Hover => Some("stripes-hover".to_string()),
    }
}

/// The effective horizontal alignment of a cell.
fn halign(cell: HorizontalAlignment, column: Option<&TableColumn>) -> &'static str {
    let alignment = if cell == HorizontalAlignment::Left {
        column.map_or(cell, TableColumn::h_align)
    } else {
        cell
    };

    match alignment {
        HorizontalAlignment::Center => "center",
        HorizontalAlignment::Right => "right",
        HorizontalAlignment::Left => "left",
    }
}

/// The effective vertical alignment of a cell.
fn valign(cell: VerticalAlignment, column: Option<&TableColumn>) -> &'static str {
    let alignment = if cell == VerticalAlignment::Top {
        column.map_or(cell, TableColumn::v_align)
    } else {
        cell
    };

    match alignment {
        VerticalAlignment::Middle => "middle",
        VerticalAlignment::Bottom => "bottom",
        VerticalAlignment::Top => "top",
    }
}

/// A full column width: 100%, counted in the ten-thousandths of a percent that
/// [`percentage`] works in.
const FULL: u64 = 100 * 10_000;

/// One column's share of the total column width, in ten-thousandths of a
/// percent, rounded to nearest.
///
/// Percentages are carried as integers rather than floats so that a row of them
/// can be summed and [`balance`]d without rounding error creeping in.
fn percentage(width: usize, total: usize) -> u64 {
    let width = u64::try_from(width).unwrap_or(u64::MAX);
    let total = u64::try_from(total).unwrap_or(u64::MAX);

    if total == 0 {
        return 0;
    }

    // Saturating, because a nonsensically wide column should pin to the full
    // width rather than wrap around.
    let scaled = width.saturating_mul(FULL);

    (scaled + total / 2) / total
}

/// Give the last measured column whatever the others' rounding left over, so
/// that the shares total exactly [`FULL`].
fn balance(shares: &mut [Option<u64>]) {
    let total: u64 = shares.iter().flatten().sum();

    let Some(last) = shares.iter_mut().flatten().next_back() else {
        return;
    };

    *last = last.saturating_add(FULL).saturating_sub(total);
}

/// Format a column width, dropping the trailing zeros a fixed precision leaves.
fn trim_percent(share: u64) -> String {
    let formatted = format!("{}.{:04}", share / 10_000, share % 10_000);
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');

    trimmed.to_string()
}

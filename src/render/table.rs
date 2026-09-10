//! Table rendering.

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
        let mut classes = self.wrapper_classes(block, "tableblock");
        classes.push(frame_class(table.frame()));
        classes.push(grid_class(table.grid()));

        if let Some(stripes) = stripes_class(table.stripes()) {
            classes.push(stripes);
        }

        classes.push(
            if table.is_autowidth() {
                "fit-content"
            } else {
                "stretch"
            }
            .to_string(),
        );

        let mut open = String::from("<table");

        if let Some(id) = block.id() {
            open.push_str(&format!(" id=\"{}\"", escape_attr(id)));
        }

        open.push_str(&format!(" class=\"{}\"", escape_attr(&classes.join(" "))));

        // An explicit `[width=NN%]` narrows the table; autowidth lets the
        // browser size it from the content instead.
        if let Some(width) = table.width().filter(|_| !table.is_autowidth()) {
            open.push_str(&format!(" style=\"width: {width}%;\""));
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

        for column in columns {
            if table.is_autowidth() || total == 0 || column.is_autowidth() {
                self.out.line("<col>");
            } else {
                let percent = (column.width() as f64) * 100.0 / (total as f64);
                self.out.line(&format!(
                    "<col style=\"width: {}%;\">",
                    trim_percent(percent)
                ));
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
            open.push_str(&format!(" colspan=\"{}\"", cell.colspan()));
        }

        if cell.rowspan() > 1 {
            open.push_str(&format!(" rowspan=\"{}\"", cell.rowspan()));
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
            // render exactly as they would at the top level.
            TableCellContent::AsciiDoc(nested) => {
                self.out.newline();
                self.out.open("div", None, &["content"]);
                self.blocks(nested.blocks().iter());
                self.out.close("div");
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
        _ => "left",
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
        _ => "top",
    }
}

/// Format a column width, dropping the trailing zeros a fixed precision leaves.
fn trim_percent(percent: f64) -> String {
    let formatted = format!("{percent:.4}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');

    trimmed.to_string()
}

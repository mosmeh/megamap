use anyhow::Result;
use crossterm::style::{self, Color};
use itertools::Itertools;
use lazy_static::lazy_static;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

lazy_static! {
    static ref SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_nonewlines();
    static ref THEME: Theme = {
        static DEFAULT_THEME_FILE: &[u8] =
            include_bytes!("../themes/sublime-monokai-extended/Monokai Extended.tmTheme");

        let mut reader = io::Cursor::new(DEFAULT_THEME_FILE);
        ThemeSet::load_from_reader(&mut reader).unwrap_or_else(|_| {
            let theme_set = ThemeSet::load_defaults();
            theme_set.themes["base16-ocean.dark"].clone()
        })
    };
}

pub struct PrinterBuilder {
    language: Option<String>,
    columns: usize,
    tabs: usize,
    true_color: bool,
}

impl Default for PrinterBuilder {
    fn default() -> Self {
        Self {
            language: None,
            columns: usize::MAX,
            tabs: 4,
            true_color: false,
        }
    }
}

impl PrinterBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn build(&self) -> Printer {
        Printer {
            language: self.language.clone(),
            columns: self.columns,
            tabs: self.tabs,
            true_color: self.true_color,
        }
    }

    pub fn language(&mut self, language: &str) -> &mut Self {
        self.language = Some(language.to_string());
        self
    }

    pub fn columns(&mut self, columns: usize) -> &mut Self {
        self.columns = columns;
        self
    }

    pub fn tabs(&mut self, tabs: usize) -> &mut Self {
        self.tabs = tabs;
        self
    }

    pub fn true_color(&mut self, yes: bool) -> &mut Self {
        self.true_color = yes;
        self
    }
}

pub struct Printer {
    language: Option<String>,
    columns: usize,
    tabs: usize,
    true_color: bool,
}

impl Printer {
    pub fn print_file<W, P>(&self, writer: &mut W, path: P) -> Result<()>
    where
        W: Write,
        P: AsRef<Path>,
    {
        let file = File::open(&path)?;
        let input_reader = InputReader::new(BufReader::new(file))?;

        let syntax = if let Some(lang) = &self.language {
            SYNTAX_SET.find_syntax_by_token(lang)
        } else {
            SYNTAX_SET.find_syntax_for_file(path)?
        }
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &THEME);

        self.print(writer, input_reader, &mut highlighter)
    }

    pub fn print_from_reader<W, R>(&self, writer: &mut W, reader: &mut R) -> Result<()>
    where
        W: Write,
        R: BufRead,
    {
        let input_reader = InputReader::new(reader)?;

        let syntax = if let Some(lang) = &self.language {
            SYNTAX_SET.find_syntax_by_token(lang)
        } else {
            SYNTAX_SET.find_syntax_by_first_line(input_reader.first_line())
        }
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &THEME);

        self.print(writer, input_reader, &mut highlighter)
    }

    fn print<W, R>(
        &self,
        writer: &mut W,
        mut input_reader: InputReader<R>,
        mut highlighter: &mut HighlightLines,
    ) -> Result<()>
    where
        W: Write,
        R: BufRead,
    {
        let mut buf = String::new();
        while input_reader.read_line(&mut buf)? {
            let line = if self.tabs > 0 {
                let expanded = expand_tabs(&buf, self.tabs);
                buf.clear();
                expanded
            } else {
                std::mem::take(&mut buf)
            };

            self.print_line(writer, &line, &mut highlighter)?;

            crossterm::queue!(writer, style::ResetColor)?;
            writeln!(writer)?;
        }

        Ok(())
    }

    fn print_line<W: Write>(
        &self,
        writer: &mut W,
        line: &str,
        highlighter: &mut HighlightLines,
    ) -> Result<()> {
        let regions = highlighter.highlight(&line, &SYNTAX_SET);

        let mut printed_columns = 0;
        for (style, region) in regions {
            let color = convert_color(&style.foreground, self.true_color);

            for (whitespace, group) in &region.chars().group_by(|c| c.is_whitespace()) {
                let text: String = group.collect();
                let width = text.width().min(self.columns - printed_columns);

                if whitespace {
                    let mut count = 0;
                    let text: String = text
                        .chars()
                        .take_while(|c| {
                            count += c.width().unwrap_or(0);
                            count <= width
                        })
                        .collect();
                    crossterm::queue!(writer, style::ResetColor, style::Print(text))?;
                } else {
                    crossterm::queue!(
                        writer,
                        style::SetForegroundColor(color),
                        style::Print("▀".repeat(width))
                    )?;
                }

                if printed_columns + width >= self.columns {
                    return Ok(());
                } else {
                    printed_columns += width;
                }
            }
        }

        Ok(())
    }
}

struct InputReader<R: BufRead> {
    inner: R,
    first_line: String,
}

impl<R: BufRead> InputReader<R> {
    fn new(mut reader: R) -> io::Result<Self> {
        let mut first_line = String::new();
        reader.read_line(&mut first_line)?;
        first_line = first_line.trim_end_matches('\n').to_string();

        let reader = InputReader {
            inner: reader,
            first_line,
        };
        Ok(reader)
    }

    fn first_line(&self) -> &str {
        &self.first_line
    }

    fn read_line(&mut self, buf: &mut String) -> io::Result<bool> {
        if self.first_line.is_empty() {
            let bytes = self.inner.read_line(buf)?;
            *buf = buf.trim_end_matches('\n').to_string();
            Ok(bytes > 0)
        } else {
            buf.push_str(&self.first_line);
            self.first_line.clear();
            Ok(true)
        }
    }
}

fn expand_tabs(mut line: &str, tab_width: usize) -> String {
    let mut buf = String::with_capacity(line.len() * 2);
    let mut cursor = 0;

    while let Some(index) = line.find('\t') {
        if index > 0 {
            cursor += index;
            buf.push_str(&line[..index]);
        }

        let spaces = tab_width - (cursor % tab_width);
        cursor += spaces;
        buf.push_str(&" ".repeat(spaces));

        line = &line[index + 1..];
    }

    buf.push_str(line);

    buf
}

fn convert_color(color: &syntect::highlighting::Color, true_color: bool) -> Color {
    if color.a == 0 {
        Color::Reset
    } else if true_color {
        Color::Rgb {
            r: color.r,
            g: color.g,
            b: color.b,
        }
    } else {
        let ansi_color = ansi_colours::ansi256_from_rgb((color.r, color.g, color.b));
        Color::AnsiValue(ansi_color)
    }
}

use crate::error;

use crossbeam::channel::{select, Receiver};
use regex::{self, Regex};
use std::fmt;
use std::io;
use termion::clear;
use termion::color;
use termion::event::Key;
use termion::style;
use termion::terminal_size;

pub type Result<T> = std::result::Result<T, error::AppError>;

#[derive(Debug, Copy, Clone)]
pub enum Mode {
    Normal,
    Search,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub struct App<R: io::BufRead, W: io::Write> {
    raw_buffer: Vec<String>,
    input: R,
    output: W,
    keys: Receiver<Key>,
    query: Vec<char>,
    mode: Mode,
}

impl<R, W> App<R, W>
where
    R: io::BufRead,
    W: io::Write,
{
    ///
    pub fn new(input: R, output: W, keys: Receiver<Key>) -> Self {
        App {
            raw_buffer: Vec::new(),
            input: input,
            output: output,
            keys: keys,
            query: Vec::new(),
            mode: Mode::Normal,
        }
    }

    // Read events
    fn iterate_over_keys(&mut self) -> Result<()> {
        loop {
            select! {
                recv(self.keys) -> key => {
                     match key {
                        Ok(key) => {
                            match (self.mode, key) {
                                // No mather which mode, ctrl-c will stop the program.
                                (_, Key::Ctrl('c')) => {
                                    return Ok(())
                                },

                                // Going into search mode.
                                (Mode::Normal, Key::Char('/')) => {
                                    self.mode = Mode::Search;
                                },

                                // We don't support multi-line search.
                                (Mode::Search, Key::Char('\n')) => {}
                                (Mode::Search, Key::Backspace) => {
                                    self.query.pop();
                                    self.redraw()?
                                },

                                (Mode::Search, Key::Char(n)) => {
                                    self.query.push(n);
                                    self.redraw()?
                                },

                                // Leaving search mode.
                                (Mode::Search, Key::Esc) => {
                                    self.mode = Mode::Normal;
                                    self.query = Vec::new();
                                    self.redraw()?
                                },

                                (_, _) => {},
                            }
                        }
                        Err(e) => {
                            return Err(error::AppError::InputError(format!("failed to receive key: {}", e).to_owned()))
                        },
                    }
                }
            }
        }
    }

    // Return a footer that is as wide as the output is. The footer is a single line that spans
    // the width of the shell.  The query that has been searched for is left on the line, while the
    // current mode is printed at the right corner. It looks something like this.
    //
    //      <query> .........<mode>
    fn footer(&self, width: usize) -> String {
        let mut footer = String::new();
        let mode = &self.mode.to_string();
        for c in self.query.clone() {
            footer.push(c);
        }

        let padding = vec![' '; width - self.query.len() - mode.chars().count() - 1];
        for c in padding {
            footer.push(c);
        }

        footer.push_str(mode);

        footer
    }

    pub fn start(&mut self) -> Result<()> {
        //write!(self.output, "{}", clear::All)?;
        //self.output.flush()?;

        self.read_input()?;
        self.iterate_over_keys()
    }

    fn read_input(&mut self) -> Result<()> {
        loop {
            let mut line = String::new();
            let n = self
                .input
                .read_line(&mut line)
                .expect("Failed to read input");

            if n == 0 {
                return Ok(());
            }
            self.raw_buffer.push(line);
            self.redraw()?;
        }
    }

    fn redraw(&mut self) -> Result<()> {
        let (width, height) = terminal_size().unwrap();
        write!(self.output, "{}", clear::All)?;
        self.output.flush()?;

        let mut regex = String::new();
        for c in self.query.clone() {
            regex.push(c);
        }

        let regex = format!(r"(.*)(?P<m>{})(.*)", regex);
        let re = Regex::new(&regex.as_str()).unwrap();

        for (i, line) in self.raw_buffer.iter().rev().enumerate() {
            if re.is_match(line) {
                if i >= height as usize {
                    break;
                }

                for cap in re.captures_iter(line) {
                    write!(
                        self.output,
                        "{}{}{}{}{}{}",
                        termion::cursor::Goto(1, height - i as u16),
                        &cap[1],
                        color::Fg(color::Red),
                        &cap[2],
                        color::Fg(color::Reset),
                        &cap[3],
                    )?;
                }
                continue;
            }

            write!(
                self.output,
                "{}{}",
                termion::cursor::Goto(1, height - i as u16),
                line
            )
            .unwrap()
        }
        self.output.flush().unwrap();
        let footer = self.footer(width as usize);

        write!(
            self.output,
            "{}{}{}{}",
            termion::cursor::Goto(1, height),
            style::Invert,
            footer,
            style::Reset
        )?;
        self.output.flush()?;

        Ok(())
    }
}

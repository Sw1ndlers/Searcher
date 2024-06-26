use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
};

use inquire::{Select, Text};
use rayon::iter::{ParallelBridge, ParallelIterator};

use crate::{
    matcher::matcher::Matcher,
    searcher::top_matches::get_top_matches,
    utils::{clear_screen::clear_screen, str_ext::StrExt},
};

use super::after_search::AfterSearchOption;

pub struct Searcher {
    base_dir: PathBuf,
    matcher: Matcher,
    verbose: bool,
    matches: Arc<Mutex<Vec<(i64, String)>>>,
    last_printed: Arc<Mutex<Vec<String>>>,
}

impl Searcher {
    pub fn new(base_dir: PathBuf, query: String, verbose: bool) -> Self {
        Self {
            base_dir,
            verbose,
            matches: Arc::new(Mutex::new(Vec::new())),
            matcher: Matcher::new(query),
            last_printed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn check_match(&self, path: &Path, _is_dir: bool) {
        let base_dir = &self.base_dir;
        let matcher = &self.matcher;

        let file_name = path.file_name().unwrap().to_str().unwrap();

        let relative_path = path.strip_prefix(base_dir).unwrap();
        let parent_dir = relative_path.parent().unwrap().to_str().unwrap();

        if let Some((score, indices)) = matcher.fmatch(file_name) {
            let colored_name = file_name.colorize_matches(indices);

            let formatted_string = format!(".\\{}\\{}", parent_dir, colored_name);

            let mut matches = self.matches.lock().unwrap();

            matches.push((score, formatted_string));
        }
    }

    fn search_directory(&self, path: &Path) -> anyhow::Result<()> {
        let Ok(children) = std::fs::read_dir(path) else {
            if self.verbose {
                println!("Error reading directory: {:?}", path);
            }

            return Ok(());
        };

        children
            .map(|entry| (entry.unwrap().path()))
            .par_bridge()
            .for_each(|path| {
                let is_dir = path.is_dir();

                self.check_match(&path, is_dir);

                if is_dir {
                    self.search_directory(&path).unwrap();
                }
            });

        anyhow::Ok(())
    }

    fn show_all(&self) {
        let matches = self.matches.lock().unwrap();
        let matches = matches
            .iter()
            .map(|(_, path)| path.to_string())
            .collect::<Vec<String>>();

        println!();

        clear_screen();

        println!("All Matches ({}):", matches.len());
        println!("{}", matches.join("\n"));
    }

    fn filter(&self) {
        let query = Text::new("Filter by:").prompt().unwrap();
        let matches = self.matches.lock().unwrap();

        let matches = matches
            .iter()
            .filter(|(_, path)| path.contains(&query))
            .map(|(_, path)| path.to_string())
            .collect::<Vec<String>>();

            println!();

        clear_screen();

        println!("Filtered Matches ({}):", matches.len());
        println!("{}", matches.join("\n"));
    }

    fn after_search(&self) -> anyhow::Result<()> {
        let answer = Select::new("Options:", AfterSearchOption::VARIANTS.to_vec()).prompt()?;
        let answer = AfterSearchOption::from_str(answer).unwrap();

        match answer {
            AfterSearchOption::ShowAll => self.show_all(),
            AfterSearchOption::Filter => self.filter(),
        }

        Ok(())
    }

    pub fn search(&self, path: &Path) -> anyhow::Result<()> {
        let start = std::time::Instant::now();

        let matches = Arc::clone(&self.matches);
        let last_printed = Arc::clone(&self.last_printed);

        let completed_search = Arc::new(Mutex::new(false));
        let completed_search_clone = Arc::clone(&completed_search);

        thread::spawn(move || {
            let mut last_printed = last_printed.lock().unwrap();

            loop {
                let matches_ref = matches.lock().unwrap();
                let mut matches = matches_ref.clone();
                let completed_search_clone = completed_search_clone.lock().unwrap();

                if *completed_search_clone {
                    break;
                }

                drop(matches_ref);

                let (matches, extra_matches) = get_top_matches(&mut matches);

                if matches == *last_printed {
                    print!("\r... {} more matches", extra_matches);
                    continue;
                }

                clear_screen();
                println!("{}", matches.join("\n"));

                *last_printed = matches;
            }
        });

        self.search_directory(path).unwrap();
        *completed_search.lock().unwrap() = true;

        let matches_ref = self.matches.lock().unwrap();
        let (matches, extra_matches) = get_top_matches(&mut matches_ref.clone());

        clear_screen();

        println!("{}", matches.join("\n"));
        println!(
            "... {} more matches in {:?}\n",
            extra_matches,
            start.elapsed()
        );

        drop(matches_ref);

        self.after_search()?;

        Ok(())
    }
}

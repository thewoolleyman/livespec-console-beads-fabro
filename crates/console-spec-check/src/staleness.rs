//! Diff-aware fenced-block staleness detection.
//!
//! [`extract_rules`](crate::extract_rules) SKIPS fenced code blocks, so a
//! mermaid diagram or a Gherkin scenario carries no clause and no gap-id and
//! CI cannot see a normative contradiction inside one. Four of the defects
//! found across the five ratification-review rounds of the v048 amendment
//! lived in fenced blocks, and each was caught only by a human or LLM reviewer
//! sweeping every block by hand.
//!
//! All four have the same shape: prose was amended and a fenced block in the
//! same file still says the old thing. That is detectable WITHOUT
//! understanding either side, which is what this module does. When a change
//! removes or rewords a normative clause line, the distinctive terms of the
//! REMOVED text are taken and any fenced block in the same file that still
//! carries one -- and that the change did not itself touch -- is reported.
//!
//! DELIBERATE LIMIT: this catches STALE blocks, not blocks that were wrong
//! when first written. Every defect this epic has paid for was a stale one,
//! and a broader check needs an LLM in the check path, which this repository's
//! checks deliberately avoid.

use std::collections::HashSet;
use std::path::Path;

use crate::has_rule_keyword;

/// A fenced block that still carries a distinctive term from a normative
/// clause line the change removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleFencedBlock {
    /// Spec file field.
    pub spec_file: String,
    /// Line number of the block's opening fence (1-based).
    pub start_line: usize,
    /// Line number of the block's closing fence (1-based).
    pub end_line: usize,
    /// The removed term the block still carries, as its normalized words.
    pub term: String,
}

impl StaleFencedBlock {
    /// This finding as one diagnostic line, in `console-spec-check`'s existing
    /// style: the severity `label`, what happened, the offending file in
    /// brackets, the block's fence line range, and — after the `::` the other
    /// diagnostics use for the offending text — the removed term the block
    /// still carries.
    ///
    /// All three are load-bearing. The file and the line range are what let a
    /// reader open the block instead of sweeping every block in the tree, and
    /// the term is what tells them WHICH removed sentence the block now
    /// contradicts.
    #[must_use]
    pub fn render(&self, label: &str) -> String {
        format!(
            "{label}: fenced block still states text the change removed [{}] lines {}-{} :: {}",
            self.spec_file, self.start_line, self.end_line, self.term
        )
    }
}

/// One spec file as a change carries it: the content before, and the content
/// now.
///
/// The binary builds these from git — `previous` is the file at the comparison
/// base, `current` is the working tree's copy — so a change set covers both
/// what the branch committed and what is still uncommitted. A file the change
/// ADDED or DELETED is not an entry at all: with only one side there is no
/// removed prose, so nothing in it can have gone stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecChange {
    /// Repository-relative path of the spec file.
    pub spec_file: String,
    /// The file's content at the comparison base.
    pub previous: String,
    /// The file's content in the working tree.
    pub current: String,
}

/// Every stale fenced block across `changes`, in change-set order and then in
/// [`stale_fenced_blocks`] order within each file.
#[must_use]
pub fn stale_fenced_blocks_in_changes(changes: &[SpecChange]) -> Vec<StaleFencedBlock> {
    changes
        .iter()
        .flat_map(|change| {
            stale_fenced_blocks(&change.spec_file, &change.previous, &change.current)
        })
        .collect()
}

/// Whether `path` names a spec markdown file this check reads.
///
/// The WHOLE `SPECIFICATION/` tree, history included. A proposed change is
/// amended round after round in its own file under `history/`, which is where
/// the v048 defects were actually introduced, and widening the scan costs
/// nothing on the rest of the tree: only files the change MODIFIED reach the
/// detector, and a file with no removed normative prose yields no findings.
#[must_use]
pub fn is_spec_markdown(path: &str) -> bool {
    path.starts_with("SPECIFICATION/")
        && Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

/// The fenced blocks of `current` that still carry a distinctive term from a
/// normative clause line `previous` had and `current` does not.
///
/// Pure function of its two inputs. A file whose prose did not change yields
/// no findings at all: with nothing removed there are no terms, so the check
/// is silent on the blocks that are merely present. A block the change itself
/// rewrote is likewise never reported -- it is by construction not stale.
///
/// One finding is emitted per (block, term) pair, in block order and then in
/// the document order of the removed terms.
#[must_use]
pub fn stale_fenced_blocks(
    spec_file: &str,
    previous: &str,
    current: &str,
) -> Vec<StaleFencedBlock> {
    let previous_document = partition(previous);
    let current_document = partition(current);
    let terms = removed_terms(&previous_document, &current_document);

    let mut findings = Vec::new();
    for block in &current_document.blocks {
        if !is_untouched(block, &previous_document.blocks) {
            continue;
        }
        let carried = block_terms(block);
        findings.extend(
            terms
                .iter()
                .filter(|term| carried.contains(*term))
                .map(|term| StaleFencedBlock {
                    spec_file: spec_file.to_owned(),
                    start_line: block.start_line,
                    end_line: block.end_line,
                    term: term.clone(),
                }),
        );
    }
    findings
}

// ---------------------------------------------------------------------------
// Document partitioning — prose lines vs fenced blocks.
// ---------------------------------------------------------------------------

/// A markdown file split into the prose the clause extractor reads and the
/// fenced blocks it skips.
struct Document {
    prose: Vec<String>,
    blocks: Vec<FencedBlock>,
}

/// One fenced block, with the 1-based line numbers of its fences.
struct FencedBlock {
    start_line: usize,
    end_line: usize,
    language: String,
    lines: Vec<String>,
}

/// Split `text` into prose lines and fenced blocks, using the same fence rule
/// as [`extract_rules`](crate::extract_rules): a line whose first non-space
/// characters are three backticks toggles the fence. An unterminated fence
/// closes at the end of the file.
fn partition(text: &str) -> Document {
    let mut prose = Vec::new();
    let mut blocks = Vec::new();
    let mut open: Option<FencedBlock> = None;
    let mut last_line = 0;
    for (index, raw_line) in text.split('\n').enumerate() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        last_line = index + 1;
        let fence = line.trim_start();
        if fence.starts_with("```") {
            match open.take() {
                Some(block) => blocks.push(FencedBlock {
                    end_line: last_line,
                    ..block
                }),
                None => {
                    open = Some(FencedBlock {
                        start_line: last_line,
                        end_line: last_line,
                        language: fence.trim_start_matches('`').trim().to_owned(),
                        lines: Vec::new(),
                    });
                }
            }
            continue;
        }
        match open.as_mut() {
            Some(block) => block.lines.push(line.to_owned()),
            None => prose.push(line.to_owned()),
        }
    }
    if let Some(block) = open {
        blocks.push(FencedBlock {
            end_line: last_line,
            ..block
        });
    }
    Document { prose, blocks }
}

/// Whether the change left this block alone. Identity is the block's language
/// plus its trimmed body, so a reindented but otherwise identical block still
/// counts as untouched.
fn is_untouched(block: &FencedBlock, previous: &[FencedBlock]) -> bool {
    let body = block_identity(block);
    previous
        .iter()
        .any(|candidate| block_identity(candidate) == body)
}

fn block_identity(block: &FencedBlock) -> String {
    let mut identity = block.language.clone();
    for line in &block.lines {
        identity.push('\n');
        identity.push_str(line.trim());
    }
    identity
}

// ---------------------------------------------------------------------------
// The removed terms — what the change stopped saying.
// ---------------------------------------------------------------------------

/// The distinctive terms of the normative prose `current` no longer carries.
///
/// A previous prose line is REMOVED when its trimmed text appears nowhere in
/// the current prose; consecutive removed lines form one run, because a
/// normative clause wraps across several lines and only the whole run carries
/// its keyword. Runs with no `MUST` / `SHOULD` keyword are not normative and
/// are ignored. A term the current prose still states is not removed at all,
/// so it is dropped -- that filter is what keeps a reworded clause from
/// flagging every block that legitimately restates it. The current prose is
/// read in paragraphs for exactly the same reason the removed lines are: a
/// term must not survive the filter merely because the two words it pairs sit
/// either side of a wrap.
fn removed_terms(previous: &Document, current: &Document) -> Vec<String> {
    let kept: HashSet<&str> = current.prose.iter().map(|line| line.trim()).collect();
    let runs = joined_runs(&previous.prose, |line| !kept.contains(line));
    let still_stated: HashSet<String> = joined_runs(&current.prose, |_| true)
        .iter()
        .flat_map(|paragraph| phrase_terms(paragraph))
        .collect();
    let mut seen = HashSet::new();
    runs.iter()
        .filter(|text| has_rule_keyword(text))
        .flat_map(|text| phrase_terms(text))
        .filter(|term| !still_stated.contains(term))
        .filter(|term| seen.insert(term.clone()))
        .collect()
}

/// Group the consecutive selected lines of `lines` into space-joined runs. A
/// blank line always ends a run.
fn joined_runs(lines: &[String], select: impl Fn(&str) -> bool) -> Vec<String> {
    let mut runs: Vec<String> = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || !select(trimmed) {
            flush_run(&mut run, &mut runs);
            continue;
        }
        run.push(trimmed);
    }
    flush_run(&mut run, &mut runs);
    runs
}

fn flush_run(run: &mut Vec<&str>, runs: &mut Vec<String>) {
    if !run.is_empty() {
        runs.push(run.join(" "));
        run.clear();
    }
}

// ---------------------------------------------------------------------------
// The block side — what a block still says.
// ---------------------------------------------------------------------------

/// The terms a fenced block carries.
///
/// A mermaid block is read line by line so that a node's identifier is not
/// confused with its label, and an EDGE line is read WHOLE -- identifiers
/// included. The round-4 v048 defect lived on an edge (`OP --> Bare -->
/// HostBd --> Beads`) whose node labels read fine in isolation, so a
/// label-only reader would miss the very case this check exists for.
fn block_terms(block: &FencedBlock) -> HashSet<String> {
    if block.language.eq_ignore_ascii_case("mermaid") {
        return block
            .lines
            .iter()
            .flat_map(|line| phrase_terms(&mermaid_content(line)))
            .collect();
    }
    phrase_terms(&block.lines.join(" ")).into_iter().collect()
}

/// The readable content of one mermaid line.
///
/// A node or subgraph DECLARATION carries its content in the label between
/// its shape delimiters; the identifier in front of the label is a
/// diagram-internal handle, not a claim, so only the label is read. Every
/// other line -- an EDGE, a `flowchart` header, an `end` -- has no label to
/// separate out and is read whole, so an edge's node identifiers are read as
/// content.
fn mermaid_content(line: &str) -> String {
    let trimmed = line.trim();
    let Some(start) = trimmed.find(['[', '(', '{']) else {
        return trimmed.to_owned();
    };
    let Some(end) = trimmed.rfind([']', ')', '}']) else {
        return trimmed.to_owned();
    };
    if end <= start + 1 {
        return trimmed.to_owned();
    }
    trimmed[start + 1..end].to_owned()
}

// ---------------------------------------------------------------------------
// Term extraction — distinctive word pairs within one clause.
// ---------------------------------------------------------------------------

/// How far apart two significant words may sit and still form a term. Two
/// allows one intervening modifier, so prose ("the host-side `bd`") and a
/// diagram's compressed form (`HostBd`) reach the same term.
const WINDOW: usize = 2;

/// Characters that end a clause. A term never spans one, which is what keeps
/// a coincidental adjacency across a punctuation break -- `work-item; never
/// fail master` -- from reading as the phrase `item never`.
const BOUNDARY_CHARS: [char; 16] = [
    '.', ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '"', '|', '<', '>',
];

/// Function words that carry no claim, so a pair built from them is not
/// distinctive. The normative modals are here too: `MUST obtain` says nothing
/// a diagram could contradict.
const STOP_WORDS: &[&str] = &[
    "about", "above", "after", "again", "against", "all", "also", "and", "any", "are", "as", "at",
    "be", "because", "been", "before", "being", "below", "between", "both", "but", "by", "can",
    "could", "did", "do", "does", "done", "down", "during", "each", "either", "every", "for",
    "from", "further", "had", "has", "have", "her", "his", "how", "if", "in", "into", "is", "it",
    "its", "may", "might", "more", "most", "must", "no", "nor", "not", "of", "on", "once", "only",
    "onto", "or", "other", "our", "out", "over", "per", "shall", "should", "since", "so", "some",
    "such", "than", "that", "the", "their", "them", "then", "there", "these", "they", "this",
    "those", "through",
];

/// Every distinctive term `text` states: the ordered pairs of significant
/// words that sit within [`WINDOW`] of each other inside one clause.
fn phrase_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for segment in segments(text) {
        let words = significant_words(&segment);
        for (index, first) in words.iter().enumerate() {
            terms.extend(
                words
                    .iter()
                    .skip(index + 1)
                    .take(WINDOW)
                    .map(|second| format!("{first} {second}")),
            );
        }
    }
    terms
}

/// Split `text` at its clause boundaries. A run of two or more hyphens is a
/// boundary too: it is this repository's em-dash AND the body of every mermaid
/// arrow, so `OP --> Bare` yields one segment per node rather than a phrase
/// spanning the arrow.
fn segments(text: &str) -> Vec<String> {
    let mut flattened = String::with_capacity(text.len());
    let mut hyphens = 0usize;
    for character in text.chars() {
        if character == '-' {
            hyphens += 1;
            continue;
        }
        push_hyphens(&mut flattened, hyphens);
        hyphens = 0;
        flattened.push(character);
    }
    push_hyphens(&mut flattened, hyphens);
    flattened
        .split(BOUNDARY_CHARS)
        .map(ToOwned::to_owned)
        .collect()
}

fn push_hyphens(out: &mut String, count: usize) {
    if count == 1 {
        out.push('-');
    } else if count > 1 {
        out.push('.');
    }
}

/// The significant words of one segment, normalized: identifiers split into
/// their words, lowercased, lightly stemmed, and stripped of function words.
fn significant_words(segment: &str) -> Vec<String> {
    words(segment)
        .iter()
        .map(|word| stem(&word.to_lowercase()))
        .filter(|word| word.len() >= 2 && !STOP_WORDS.contains(&word.as_str()))
        .collect()
}

/// Split a segment into words. Non-alphanumeric characters separate, and so
/// do the internal seams of an identifier -- a camel-case hump (`HostBd`) and
/// a digit-to-letter change (`1Password`) -- so a diagram's identifiers are
/// read as the words they compress.
fn words(segment: &str) -> Vec<String> {
    let mut collected = Vec::new();
    let mut word = String::new();
    let mut previous: Option<char> = None;
    for character in segment.chars() {
        if !character.is_alphanumeric() {
            flush_word(&mut word, &mut collected);
            previous = None;
            continue;
        }
        if previous.is_some_and(|prior| splits_identifier(prior, character)) {
            flush_word(&mut word, &mut collected);
        }
        word.push(character);
        previous = Some(character);
    }
    flush_word(&mut word, &mut collected);
    collected
}

fn splits_identifier(previous: char, character: char) -> bool {
    (!previous.is_uppercase() && character.is_uppercase())
        || (previous.is_numeric() != character.is_numeric())
}

fn flush_word(word: &mut String, collected: &mut Vec<String>) {
    if !word.is_empty() {
        collected.push(std::mem::take(word));
    }
}

/// Fold the inflections a diagram and its prose disagree on: `top-ranked` in a
/// node label against `top of the rank order` in the clause it illustrates.
/// Deliberately crude and deterministic -- it is a normalization, not a
/// linguistic claim, and it only ever costs recall.
fn stem(word: &str) -> String {
    if word.len() > 4 {
        if let Some(base) = word.strip_suffix("ing") {
            return base.to_owned();
        }
        if let Some(base) = word.strip_suffix("ed") {
            return base.to_owned();
        }
    }
    if word.len() > 3
        && !word.ends_with("ss")
        && let Some(base) = word.strip_suffix('s')
    {
        return base.to_owned();
    }
    word.to_owned()
}

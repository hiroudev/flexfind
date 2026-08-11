// Pure display-formatting helper — no query parsing here. FlexFind's query
// grammar is parsed exactly once, in Rust (src-tauri/src/index/query.rs),
// which reports the match span back on every `SearchHit`. This just slices
// the name for rendering.

export interface HighlightSpan {
  pre: string
  match: string
  post: string
}

export function splitHighlight(name: string, matchStart: number, matchLen: number): HighlightSpan {
  if (matchLen <= 0) return { pre: name, match: '', post: '' }
  return {
    pre: name.slice(0, matchStart),
    match: name.slice(matchStart, matchStart + matchLen),
    post: name.slice(matchStart + matchLen),
  }
}

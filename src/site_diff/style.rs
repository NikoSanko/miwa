//! The inline CSS of the diff page (light and dark, narrow screens stacked).

pub(super) const STYLE: &str = r#"
:root{
  --bg:#ffffff; --bg-2:#f6f8fa; --fg:#1f2328; --muted:#59636e; --border:#d1d9e0; --accent:#0969da;
  --add-bg:#dafbe1; --add-word:#aceebb; --add-g:#ccffd8; --add-fg:#1a7f37;
  --del-bg:#ffebe9; --del-word:#ffcecb; --del-g:#ffd7d5; --del-fg:#d1242f;
  --mod-fg:#9a6700; --hunk:#ddf4ff; --code:#eff1f3; --bar-0:#d1d9e0;
  --void:repeating-linear-gradient(135deg,transparent 0 5px,rgba(0,0,0,.035) 5px 10px);
}
@media (prefers-color-scheme: dark){:root{
  --bg:#0d1117; --bg-2:#151b23; --fg:#f0f6fc; --muted:#9198a1; --border:#3d444d; --accent:#4493f8;
  --add-bg:rgba(46,160,67,.15); --add-word:rgba(46,160,67,.4); --add-g:rgba(63,185,80,.3); --add-fg:#3fb950;
  --del-bg:rgba(248,81,73,.1); --del-word:rgba(248,81,73,.4); --del-g:rgba(248,81,73,.3); --del-fg:#f85149;
  --mod-fg:#d29922; --hunk:rgba(56,139,253,.1); --code:#262c36; --bar-0:#3d444d;
  --void:repeating-linear-gradient(135deg,transparent 0 5px,rgba(255,255,255,.035) 5px 10px);
}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI","Noto Sans","Hiragino Sans","Noto Sans JP",Helvetica,Arial,sans-serif}
a{color:var(--accent);text-decoration:none} a:hover{text-decoration:underline}
code{background:var(--code);padding:.1em .35em;border-radius:6px;font:85%/1.4 ui-monospace,SFMono-Regular,Menlo,monospace}
pre{background:var(--code);padding:10px 12px;overflow-x:auto;border-radius:6px} pre code{padding:0;background:none}
img{max-width:100%;height:auto}
table{border-collapse:collapse} th,td{border:1px solid var(--border);padding:4px 10px}
summary{list-style:none;cursor:pointer} summary::-webkit-details-marker{display:none}
.top{display:flex;flex-wrap:wrap;align-items:center;gap:8px 16px;padding:12px 20px;border-bottom:1px solid var(--border);background:var(--bg-2)}
.mark{width:24px;height:24px;flex:none}
.site{font-weight:600;font-size:16px}
.cmp{font:13px ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted)}
.counts{display:flex;gap:12px;margin-left:auto;font-size:13px;color:var(--muted)}
.counts>span{display:inline-flex;align-items:center;gap:4px}
.st{display:inline-flex;align-items:center;justify-content:center;width:16px;height:16px;flex:none;border:1.5px solid currentColor;border-radius:4px;font-size:12px;font-weight:700;line-height:1}
.st.add{color:var(--add-fg)} .st.del{color:var(--del-fg)} .st.mod{color:var(--mod-fg)} .st.move{color:var(--muted)}
.layout{display:grid;grid-template-columns:280px minmax(0,1fr)}
aside{position:sticky;top:0;align-self:start;height:100vh;overflow:auto;border-right:1px solid var(--border);padding:12px 8px}
main{padding:16px 20px 80px;min-width:0}
.tree,.tree ul{list-style:none;margin:0;padding:0} .tree ul{padding-left:14px}
.tree .dir{display:flex;align-items:center;gap:6px;padding:3px 8px;border-radius:6px;color:var(--fg)}
.tree .dir::before{content:"\25B8";color:var(--muted);font-size:11px;width:10px} .tree details[open]>.dir::before{content:"\25BE"}
.tree a{display:flex;align-items:center;gap:6px;padding:3px 8px 3px 24px;border-radius:6px;color:var(--fg)}
.tree a:hover,.tree .dir:hover{background:var(--bg-2);text-decoration:none}
.tree .fn{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.file{border:1px solid var(--border);border-radius:6px;margin:0 0 16px;scroll-margin-top:8px}
.fh{position:sticky;top:0;z-index:3;display:flex;align-items:center;flex-wrap:wrap;gap:4px 8px;padding:8px 12px;background:var(--bg-2);border-bottom:1px solid var(--border);border-radius:6px 6px 0 0}
.file:not([open])>.fh{border-bottom:none;border-radius:6px}
.chev{display:inline-block;color:var(--muted);font-size:12px} .file:not([open]) .chev{transform:rotate(-90deg)}
.ft{font-weight:600} .ft s{color:var(--muted);font-weight:400;text-decoration-color:var(--del-fg)}
.id{font:12px ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted)}
.fp{order:5;font:12px ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted);flex-basis:100%;padding-left:44px}
.bar{display:inline-flex;gap:1px;margin-left:auto} .bar i{width:8px;height:8px;background:var(--bar-0);border-radius:1px} .bar i.a{background:var(--add-fg)} .bar i.d{background:var(--del-fg)}
.nochange{margin:0;padding:12px 16px;color:var(--muted);font-size:13px}
.row{display:grid;grid-template-columns:24px minmax(0,1fr) 24px minmax(0,1fr)}
.row1{display:grid;grid-template-columns:24px minmax(0,1fr)}
.g{font:12px/1 ui-monospace,SFMono-Regular,Menlo,monospace;text-align:center;padding-top:.9em;color:var(--muted);user-select:none}
.c{padding:0 12px;min-width:0;overflow-wrap:anywhere}
.c>:first-child{margin-top:.5em} .c>:last-child{margin-bottom:.5em}
.c ul,.c ol{margin:.2em 0;padding-left:1.5em} .c>ul:only-child,.c>ol:only-child{margin:.1em 0}
.c h1,.c h2,.c h3,.c h4{line-height:1.35} .c h2{font-size:1.3em} .c h3{font-size:1.12em}
.row .c.r,.row .g.r{border-left:1px solid var(--border)}
.row.mod .g.l,.row.del .g.l,.row.img .g.l{background:var(--del-g);color:var(--del-fg)}
.row.mod .c.l,.row.del .c.l,.row.img .c.l{background:var(--del-bg)}
.row.mod .g.r,.row.add .g.r,.row.img .g.r{background:var(--add-g);color:var(--add-fg)}
.row.mod .c.r,.row.add .c.r,.row.img .c.r{background:var(--add-bg)}
.row1.add .g{background:var(--add-g);color:var(--add-fg)} .row1.add .c{background:var(--add-bg)}
.row1.del .g{background:var(--del-g);color:var(--del-fg)} .row1.del .c{background:var(--del-bg)}
.void{background:var(--void)}
.dd{background:var(--del-word);border-radius:3px}
.da{background:var(--add-word);border-radius:3px}
.lab{float:left;margin:.6em .6em 0 0;font:11px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted);border:1px solid var(--border);border-radius:4px;padding:0 5px;background:var(--bg)}
.fold>summary{display:block;background:var(--hunk);color:var(--muted);font-size:12px;padding:3px 12px;border-top:1px solid var(--border);border-bottom:1px solid var(--border);overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.fold>summary:hover{color:var(--accent)}
.fold[open]>summary{display:none}
.file>.split>:first-child.fold>summary{border-top:none}
.file>.split>:last-child.fold>summary{border-bottom:none;border-radius:0 0 6px 6px}
@media (max-width: 860px){
  .layout{grid-template-columns:1fr} aside{display:none}
  .counts{margin-left:0}
  .row{grid-template-columns:24px minmax(0,1fr)}
  .row .g.r,.row .c.r{border-left:none}
  .row .c.void,.row .g:has(+ .c.void){display:none}
  .row.same .g.l,.row.same .c.l{display:none}
  .fp{padding-left:0}
}
"#;

/// The site mark shown at the left of the header.
pub(super) const MARK: &str = r##"<svg class="mark" viewBox="0 0 64 64" aria-hidden="true"><rect width="64" height="64" rx="14" fill="#EEE3C7"/><path d="M10 22l22-11 22 11-22 11z" fill="#1F557A"/><path d="M10 32l22-11 22 11-22 11z" fill="#C94B3C"/><path d="M10 42l22-11 22 11-22 11z" fill="#1F557A"/></svg>"##;

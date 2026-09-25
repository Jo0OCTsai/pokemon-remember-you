//! §5.1 scope 解析（纯函数）：path ⇄ scope 标识、并集展开、白名单 fail-closed 判定。
//!
//! 冻结语义（REQUIREMENTS FR-3.1 / DESIGN §5.1、§5.3）：
//! - scope grammar：`person | domains/<seg>(/<seg>)* | apps/<seg> | projects/<seg> | journal | inbox | archive | index`
//! - `V = { person/ }`（仅当 person ∈ 白名单）`∪ 展开(声明集)`；
//!   `journal/inbox/archive/index/.cache/.obsidian` 恒不进缺省检索集——显式声明 `--scope archive`
//!   等须经白名单判定后单独放行（仅检索语义；注入管线恒排除，见 [`expand_for_inject`]）
//! - 白名单判定 fail-closed：申请的 scope 列表须全部 ⊆ 白名单（递归归属：白名单条目覆盖其子树），
//!   任一越权即整单拒绝（FR-7.3）；human 全量可读 = [`ReadGrant::Full`]
//! - `.dex-ignore` 的生效发生在遍历层（store::fs），本模块不做 I/O
//! - 非法输入（未知 scope、路径穿越段）→ E_BAD_ARGS（`..`/`.`/空段/含 `\` 一律拒绝）

use crate::errors::DexError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 四层 scope 目录（缺省检索集的骨架，FR-6.1）
pub const FOUR_LAYERS: [&str; 4] = ["person", "domains", "apps", "projects"];
/// 恒排除集（缺省检索集与注入集都不进；显式声明走放行规则）
pub const ALWAYS_EXCLUDED: [&str; 6] = [
    "journal",
    "inbox",
    "archive",
    "index",
    ".cache",
    ".obsidian",
];

/// scope 标识（validated string）
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ScopeId(String);

/// 顶层 scope 类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Person,
    Domains,
    Apps,
    Projects,
    Journal,
    Inbox,
    Archive,
    Index,
}

impl ScopeKind {
    /// 是否四层 scope 目录（可进缺省检索集/注入集）
    pub fn is_four_layer(self) -> bool {
        matches!(
            self,
            ScopeKind::Person | ScopeKind::Domains | ScopeKind::Apps | ScopeKind::Projects
        )
    }
}

fn valid_segment(seg: &str) -> bool {
    !seg.is_empty()
        && seg != "."
        && seg != ".."
        && !seg.starts_with('.')
        && !seg.contains('/')
        && !seg.contains('\\')
        && !seg.contains('\0')
}

impl ScopeId {
    /// 解析并校验 scope 标识；非法 → [`DexError::BadArgs`]（未知 scope/路径穿越段）。
    pub fn new(s: &str) -> Result<Self, DexError> {
        let s = s.trim();
        let bad = || DexError::BadArgs {
            message: format!("未知 scope：{s:?}"),
        };
        match s {
            "person" | "journal" | "inbox" | "archive" | "index" => Ok(ScopeId(s.to_string())),
            _ => {
                let (top, rest) = match s.split_once('/') {
                    Some(x) => x,
                    None => return Err(bad()),
                };
                let segs: Vec<&str> = rest.split('/').collect();
                if segs.iter().any(|s| !valid_segment(s)) {
                    return Err(bad());
                }
                match top {
                    // domains 递归：domains/<seg>(/<seg>)*，至少一段
                    "domains" if !segs.is_empty() => Ok(ScopeId(s.to_string())),
                    // apps/projects 一级：恰好一段
                    "apps" | "projects" if segs.len() == 1 => Ok(ScopeId(s.to_string())),
                    _ => Err(bad()),
                }
            }
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn kind(&self) -> ScopeKind {
        let top = self.0.split('/').next().unwrap_or("");
        match top {
            "person" => ScopeKind::Person,
            "domains" => ScopeKind::Domains,
            "apps" => ScopeKind::Apps,
            "projects" => ScopeKind::Projects,
            "journal" => ScopeKind::Journal,
            "inbox" => ScopeKind::Inbox,
            "archive" => ScopeKind::Archive,
            "index" => ScopeKind::Index,
            _ => unreachable!("ScopeId 构造已校验"),
        }
    }

    /// FR-3.2 ① 具体性：projects(3) ＞ apps(2) ＞ domains(1) ＞ person(0)
    pub fn specificity(&self) -> u8 {
        match self.kind() {
            ScopeKind::Projects => 3,
            ScopeKind::Apps => 2,
            ScopeKind::Domains => 1,
            _ => 0,
        }
    }

    /// scope 对应的目录前缀（相对仓库根）；journal/inbox/archive/index/person 为单目录。
    pub fn dir_prefix(&self) -> PathBuf {
        PathBuf::from(&self.0)
    }

    /// `self` 是否为 `other` 的子 scope（或相等）——白名单递归归属（FR-3.5）。
    pub fn is_within(&self, other: &ScopeId) -> bool {
        if self == other {
            return true;
        }
        let a: Vec<&str> = self.0.split('/').collect();
        let b: Vec<&str> = other.0.split('/').collect();
        a.len() > b.len() && a[..b.len()] == b[..]
    }
}

impl std::fmt::Display for ScopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 读授权：human 全量可读（FR-10.4）或显式 scope 白名单（FR-7.3）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadGrant {
    Full,
    Scopes(Vec<ScopeId>),
}

impl ReadGrant {
    /// `scope` 是否被授权（白名单条目覆盖其子树；journal/inbox/archive 须白名单显式含该 scope）
    pub fn allows(&self, scope: &ScopeId) -> bool {
        match self {
            ReadGrant::Full => true,
            ReadGrant::Scopes(list) => list.iter().any(|w| scope.is_within(w)),
        }
    }

    /// 从 scope 列表构造（空列表 = 仅显式声明的空授权）
    pub fn of(list: Vec<ScopeId>) -> Self {
        ReadGrant::Scopes(list)
    }
}

/// 展开结果：可遍历的相对目录前缀集合（已去重排序）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VisibleSet {
    pub dirs: Vec<PathBuf>,
}

/// 缺省检索集（FR-6.1/§5.3，CLI 与 MCP 同口径）：白名单 ∩ 四层 scope 目录。
/// human 全量时 = person + 全部 domains/apps/projects（四层根全开）。
pub fn default_search_dirs(grant: &ReadGrant) -> Vec<PathBuf> {
    match grant {
        ReadGrant::Full => FOUR_LAYERS.iter().map(PathBuf::from).collect(),
        ReadGrant::Scopes(list) => {
            let mut dirs: Vec<PathBuf> = Vec::new();
            for s in list {
                if s.kind().is_four_layer() {
                    dirs.push(s.dir_prefix());
                }
            }
            dirs.sort();
            dirs.dedup();
            dirs
        }
    }
}

/// 白名单判定（fail-closed 整单拒绝，§8.2）：申请列表须全部 ⊆ 白名单，任一越权 → E_SCOPE_DENIED。
pub fn check_scopes(requested: &[ScopeId], grant: &ReadGrant) -> Result<(), DexError> {
    for r in requested {
        if !grant.allows(r) {
            return Err(DexError::ScopeDenied {
                detail: format!("申请 scope {r} 超出客户端白名单（整单拒绝）"),
            });
        }
    }
    Ok(())
}

/// scope 并集展开（§5.1）：`V = { person/ if person ∈ W } ∪ 展开(声明集)`。
/// - `declared` 为空 → 用缺省检索集（§5.3）
/// - 显式声明的 journal/archive 等经 [`check_scopes`] 白名单判定后放行（仅检索语义）
/// - `.dex-ignore` 过滤在遍历层（store::fs）执行
pub fn expand(declared: &[ScopeId], grant: &ReadGrant) -> Result<VisibleSet, DexError> {
    if declared.is_empty() {
        // 缺省检索集（§5.3）：白名单 ∩ 四层（Full → 四层全开）；
        // default_search_dirs 已含「person 恒在仅当 ∈ 白名单」语义（白名单含 person 或 Full）
        return Ok(VisibleSet {
            dirs: default_search_dirs(grant),
        });
    }
    check_scopes(declared, grant)?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    // person 恒在并集——仅当 person ∈ 白名单（FR-3.1 条件式；human 全量恒在）
    let person = ScopeId::new("person").unwrap();
    if grant.allows(&person) {
        dirs.push(person.dir_prefix());
    }
    for s in declared {
        dirs.push(s.dir_prefix());
    }
    dirs.sort();
    dirs.dedup();
    Ok(VisibleSet { dirs })
}

/// 注入展开（§5.1 末行）：注入管线恒排除 journal/inbox/archive/index——
/// 目标客户端 scopes 中的非四层条目静默丢弃（配置不应出现；出现也不进注入）。
/// `declared` 即目标客户端自身的 scopes（其授权面本身），不做二次 fail-closed 判定；
/// `grant` 仅用于 person 恒在条件式。
pub fn expand_for_inject(declared: &[ScopeId], grant: &ReadGrant) -> VisibleSet {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let person = ScopeId::new("person").unwrap();
    if grant.allows(&person) {
        dirs.push(person.dir_prefix());
    }
    for s in declared {
        if s.kind().is_four_layer() {
            dirs.push(s.dir_prefix());
        }
    }
    dirs.sort();
    dirs.dedup();
    VisibleSet { dirs }
}

/// 路径 → scope 标识（递归归属，§2.1）：
/// - `person|journal|inbox|archive|index` 顶层（含子路径）→ 该 scope
/// - `domains/<d>/…` → `domains/<d>`（一级域即 scope 粒度）
/// - `apps/<a>/…` / `projects/<p>/…` → `apps/<a>` / `projects/<p>`
/// - 其余（顶层白名单外、`.` 开头等）→ None
pub fn scope_of_path(rel: &Path) -> Option<ScopeId> {
    let mut comps = rel.components();
    let top = comps.next()?.as_os_str().to_str()?;
    let second = comps.next().and_then(|c| c.as_os_str().to_str());
    let id = match (top, second) {
        ("person", _) => "person".to_string(),
        ("journal", _) => "journal".to_string(),
        ("inbox", _) => "inbox".to_string(),
        ("archive", _) => "archive".to_string(),
        ("index", _) => "index".to_string(),
        ("domains", Some(d)) if valid_segment(d) => format!("domains/{d}"),
        ("apps", Some(a)) if valid_segment(a) => format!("apps/{a}"),
        ("projects", Some(p)) if valid_segment(p) => format!("projects/{p}"),
        _ => return None,
    };
    ScopeId::new(&id).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(list: &[&str]) -> Vec<ScopeId> {
        list.iter().map(|s| ScopeId::new(s).unwrap()).collect()
    }

    #[test]
    fn grammar_accepts_valid_ids() {
        for ok in [
            "person",
            "journal",
            "inbox",
            "archive",
            "index",
            "domains/coding",
            "domains/coding/rust",
            "apps/todo",
            "projects/foo",
        ] {
            assert!(ScopeId::new(ok).is_ok(), "应合法：{ok}");
        }
        for bad in [
            "",
            "domains",
            "apps",
            "projects",
            "apps/a/b",
            "foo",
            "person/x",
            "domains/../person",
            "domains/.git",
            "../escape",
            "a\\b",
            "domains/",
        ] {
            assert!(ScopeId::new(bad).is_err(), "应非法：{bad:?}");
        }
    }

    #[test]
    fn specificity_order() {
        let p = ScopeId::new("person").unwrap();
        let d = ScopeId::new("domains/coding").unwrap();
        let a = ScopeId::new("apps/todo").unwrap();
        let pr = ScopeId::new("projects/foo").unwrap();
        assert!(pr.specificity() > a.specificity());
        assert!(a.specificity() > d.specificity());
        assert!(d.specificity() > p.specificity());
    }

    #[test]
    fn recursive_attribution() {
        assert_eq!(
            scope_of_path(Path::new("projects/foo/sub/deep.md"))
                .unwrap()
                .as_str(),
            "projects/foo"
        );
        assert_eq!(
            scope_of_path(Path::new("domains/people/李四.md"))
                .unwrap()
                .as_str(),
            "domains/people"
        );
        assert_eq!(
            scope_of_path(Path::new("person/profile.md"))
                .unwrap()
                .as_str(),
            "person"
        );
        assert_eq!(
            scope_of_path(Path::new("journal/2026-09-20.md"))
                .unwrap()
                .as_str(),
            "journal"
        );
        assert_eq!(scope_of_path(Path::new(".git/config")), None);
        assert_eq!(scope_of_path(Path::new("random/x.md")), None);
    }

    #[test]
    fn grant_recursive_and_fail_closed() {
        let grant = ReadGrant::of(scopes(&["person", "domains/work", "projects/foo"]));
        assert!(grant.allows(&ScopeId::new("projects/foo").unwrap()));
        assert!(grant.allows(&ScopeId::new("person").unwrap()));
        // 「projects」裸名不是合法 scope id（grammar 要求 projects/<seg>）；
        // 白名单外的同级条目不因前缀相同而放行
        assert!(!grant.allows(&ScopeId::new("projects/other").unwrap()));
        assert!(!grant.allows(&ScopeId::new("apps/todo").unwrap()));
        // 整单拒绝：任一越权即 E_SCOPE_DENIED
        let req = scopes(&["person", "apps/todo"]);
        let err = check_scopes(&req, &grant).unwrap_err();
        assert_eq!(err.code(), "E_SCOPE_DENIED");
        assert_eq!(err.exit_code(), 3);
    }

    #[test]
    fn expand_person_conditional_and_excluded() {
        // person 未授权 → 不强制并入（fail-closed，FR-3.1）
        let grant = ReadGrant::of(scopes(&["projects/foo"]));
        let v = expand(&scopes(&["projects/foo"]), &grant).unwrap();
        assert_eq!(v.dirs, vec![PathBuf::from("projects/foo")]);
        // human 全量：person 恒在 + 显式声明
        let v = expand(&scopes(&["projects/foo"]), &ReadGrant::Full).unwrap();
        assert_eq!(
            v.dirs,
            vec![PathBuf::from("person"), PathBuf::from("projects/foo")]
        );
        // 显式 archive：human 放行（仅检索语义）
        let v = expand(&scopes(&["archive"]), &ReadGrant::Full).unwrap();
        assert!(v.dirs.contains(&PathBuf::from("archive")));
        // 白名单不含 archive → 整单拒绝
        let grant = ReadGrant::of(scopes(&["person"]));
        let err = expand(&scopes(&["archive"]), &grant).unwrap_err();
        assert_eq!(err.code(), "E_SCOPE_DENIED");
    }

    #[test]
    fn default_search_set_is_whitelist_cap_four_layers() {
        let grant = ReadGrant::of(scopes(&["person", "domains/work", "journal", "apps/todo"]));
        let dirs = default_search_dirs(&grant);
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("apps/todo"),
                PathBuf::from("domains/work"),
                PathBuf::from("person")
            ]
        );
        // journal 白名单里有但不进缺省检索集（显式声明才放行）
        let grant = ReadGrant::of(scopes(&["journal"]));
        assert!(default_search_dirs(&grant).is_empty());
        let full = default_search_dirs(&ReadGrant::Full);
        assert_eq!(
            full,
            FOUR_LAYERS.iter().map(PathBuf::from).collect::<Vec<_>>()
        );
    }

    #[test]
    fn inject_expansion_drops_non_four_layers() {
        let grant = ReadGrant::of(scopes(&["person", "journal"]));
        let v = expand_for_inject(&scopes(&["person", "journal", "projects/foo"]), &grant);
        assert_eq!(
            v.dirs,
            vec![PathBuf::from("person"), PathBuf::from("projects/foo")]
        );
    }
}

//! Alle HTTP-Routen (Backend und Web nutzen dieselben Pfade).
//!
//! | Methode | Pfad | Body → Antwort |
//! |---|---|---|
//! | GET  | /api/auth/me | → AuthState |
//! | POST | /api/auth/setup, /api/auth/login | Credentials → AuthState (Cookie) |
//! | POST | /api/auth/logout | → Ok |
//! | POST | /api/auth/password | ChangePassword → Ok |
//! | GET/POST | /api/tokens | – / CreateToken → Vec<ApiToken> / CreatedToken |
//! | DELETE | /api/tokens/{tid} | → Ok |
//! | GET  | /api/overview | → Overview |
//! | GET/POST | /api/apps | – / CreateApp → Vec<App> / App |
//! | GET/PATCH/DELETE | /api/apps/{id} | – / UpdateApp / – → App / App / Ok |
//! | POST | /api/apps/{id}/power | PowerRequest → App |
//! | POST | /api/apps/{id}/deploy | → App (Build läuft im Hintergrund, Status `building`) |
//! | GET  | /api/apps/{id}/logs?lines=200 | → Logs (Container) |
//! | GET  | /api/apps/{id}/build-log | → Logs (letzter Build) |
//! | GET  | /api/apps/{id}/source | → SourceTree |
//! | POST | /api/apps/{id}/source?replace=true | multipart (`file`: zip/tar.gz oder einzelne Dateien) → SourceTree |
//! | PUT  | /api/apps/{id}/source?replace=true | Rohdaten zip oder tar.gz → SourceTree (für curl) |
//! | POST | /api/apps/{id}/ports | AddPort → App |
//! | DELETE | /api/apps/{id}/ports/{host} | → App |
//! | POST | /api/apps/{id}/exec | ExecRequest → ExecResult |
//! | POST | /api/apps/{id}/webhook-secret | → App (neue Webhook-URL) |
//! | POST | /api/hooks/{id}/{secret} | Git-Webhook → Ok |
//! | GET/PUT | /api/admin/settings | – / Settings → Settings |
//! | GET  | /api/admin/system | → SystemInfo |
//! | GET/POST | /api/admin/users | – / CreateUser → Vec<AdminUser> / AdminUser |
//! | PATCH/DELETE | /api/admin/users/{uid} | UpdateUser → AdminUser / Ok |
//! | GET  | /api/admin/apps | → Vec<App> (alle Benutzer) |
//! | POST | /api/admin/cleanup | → CleanupResult |
//! | POST | /mcp | MCP (JSON-RPC, Bearer-Token) |
//! | GET  | /llm.txt | Anleitung für KIs (Text) |

pub fn app(id: &str, rest: &str) -> String {
    if rest.is_empty() {
        format!("/api/apps/{id}")
    } else {
        format!("/api/apps/{id}/{rest}")
    }
}

pub fn admin_user(uid: &str) -> String {
    format!("/api/admin/users/{uid}")
}

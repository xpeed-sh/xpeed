use eframe::egui::{self, Color32, Rect};
#[allow(clippy::excessive_precision)]
#[path = "icon_meshes.rs"]
mod meshes;

#[derive(Clone, Copy)]
pub enum Icon {
    Settings,
    Workspace,
    Extensions,
    SidebarLeft,
    SidebarRight,
    Connection,
    Logo,
    Moon,
    Sun,
    File,
    Folder,
    Image,
    Audio,
    Video,
    Minimize,
    Maximize,
    Close,
    Home,
    Info,
    Copy,
    CopyChecked,
    RequestApproval,
    FullAccess,
    AccessSelected,
    Trash,
}

pub fn logo_size(height: f32) -> egui::Vec2 {
    egui::vec2(height * meshes::LOGO_ASPECT, height)
}

pub fn draw(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let layers = match icon {
        Icon::Settings => meshes::SETTINGS,
        Icon::Workspace => meshes::WORKSPACE,
        Icon::Extensions => meshes::EXTENSIONS,
        Icon::SidebarLeft => meshes::SIDEBAR_LEFT,
        Icon::SidebarRight => meshes::SIDEBAR_RIGHT,
        Icon::Connection => meshes::CONNECTION,
        Icon::Logo => meshes::LOGO,
        Icon::Moon => meshes::MOON,
        Icon::Sun => meshes::SUN,
        Icon::File => meshes::FILE,
        Icon::Folder => meshes::FOLDER,
        Icon::Image => meshes::IMAGE,
        Icon::Audio => meshes::AUDIO,
        Icon::Video => meshes::VIDEO,
        Icon::Minimize => meshes::MINIMIZE,
        Icon::Maximize => meshes::MAXIMIZE,
        Icon::Close => meshes::CLOSE,
        Icon::Home => meshes::HOME,
        Icon::Info => meshes::INFO,
        Icon::Copy => meshes::COPY,
        Icon::CopyChecked => meshes::COPY_CHECKED,
        Icon::RequestApproval => meshes::REQUEST_APPROVAL,
        Icon::FullAccess => meshes::FULL_ACCESS,
        Icon::AccessSelected => meshes::ACCESS_SELECTED,
        Icon::Trash => meshes::TRASH,
    };
    for &(opacity, points, indices) in layers {
        let mut mesh = egui::Mesh::default();
        let tint = color.gamma_multiply(opacity);
        for point in points {
            mesh.colored_vertex(
                rect.min
                    + egui::vec2(
                        point[0] * rect.width() / 24.0,
                        point[1] * rect.height() / 24.0,
                    ),
                tint,
            );
        }
        mesh.indices.extend_from_slice(indices);
        painter.add(egui::Shape::mesh(mesh));
    }
}

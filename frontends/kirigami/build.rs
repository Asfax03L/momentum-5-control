//! Compile le pont Rust ↔ QML (cxx-qt) et le module QML de l'interface.
//! Sous Linux seulement : Qt 6 (qmake6) doit être installé.

#[cfg(target_os = "linux")]
fn main() {
    use cxx_qt_build::{CxxQtBuilder, QmlModule};
    CxxQtBuilder::new_qml_module(
        QmlModule::new("org.openheadset.control").qml_files(["qml/Main.qml"]),
    )
    .files(["src/bridge.rs"])
    .cpp_files(["src/identity.cpp"])
    .build();
}

#[cfg(not(target_os = "linux"))]
fn main() {}

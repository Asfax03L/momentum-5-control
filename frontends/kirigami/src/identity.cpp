// Sous Wayland, KDE associe une fenêtre à son fichier .desktop par son
// identifiant, et le gestionnaire de tâches prend l'icône transmise par la
// fenêtre : sans icône explicite, Qt envoie « aucune icône » et la barre
// des tâches affiche une icône vide. On les fixe donc tous les deux.
#include "momentum-kirigami/src/identity.h"

#include <QGuiApplication>
#include <QIcon>
#include <QColor>
#include <QPalette>
#include <QSettings>
#include <QStandardPaths>
#include <QStringList>
#include <QStyleHints>

void openheadset_set_identity()
{
    // Dans le Flatpak, le fichier .desktop porte l'identifiant de l'application.
    QGuiApplication::setDesktopFileName(
        qEnvironmentVariable("FLATPAK_ID", QStringLiteral("momentum-kirigami")));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("OpenHeadset Control"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.openheadset.control"),
                                                    QIcon::fromTheme(QStringLiteral("audio-headphones"))));
}

// Palette Qt lue dans un jeu de couleurs KDE (fichier .colors, format INI).
static QPalette palette_from_scheme(const QString &path)
{
    QSettings scheme(path, QSettings::IniFormat);
    QPalette palette = QGuiApplication::palette();
    const auto color = [&scheme](const QString &group, const QString &key) {
        const QStringList rgb = scheme.value(group + QLatin1Char('/') + key).toStringList();
        return rgb.size() >= 3 ? QColor(rgb[0].toInt(), rgb[1].toInt(), rgb[2].toInt()) : QColor();
    };
    const auto set = [&palette](QPalette::ColorRole role, const QColor &c) {
        if (c.isValid()) {
            palette.setColor(QPalette::All, role, c);
        }
    };
    const QString window = QStringLiteral("Colors:Window");
    const QString view = QStringLiteral("Colors:View");
    const QString button = QStringLiteral("Colors:Button");
    const QString selection = QStringLiteral("Colors:Selection");
    const QString tooltip = QStringLiteral("Colors:Tooltip");
    set(QPalette::Window, color(window, QStringLiteral("BackgroundNormal")));
    set(QPalette::WindowText, color(window, QStringLiteral("ForegroundNormal")));
    set(QPalette::Base, color(view, QStringLiteral("BackgroundNormal")));
    set(QPalette::AlternateBase, color(view, QStringLiteral("BackgroundAlternate")));
    set(QPalette::Text, color(view, QStringLiteral("ForegroundNormal")));
    set(QPalette::Button, color(button, QStringLiteral("BackgroundNormal")));
    set(QPalette::ButtonText, color(button, QStringLiteral("ForegroundNormal")));
    set(QPalette::Highlight, color(selection, QStringLiteral("BackgroundNormal")));
    set(QPalette::HighlightedText, color(selection, QStringLiteral("ForegroundNormal")));
    set(QPalette::ToolTipBase, color(tooltip, QStringLiteral("BackgroundNormal")));
    set(QPalette::ToolTipText, color(tooltip, QStringLiteral("ForegroundNormal")));
    set(QPalette::Link, color(view, QStringLiteral("ForegroundLink")));
    set(QPalette::PlaceholderText, color(view, QStringLiteral("ForegroundInactive")));
    return palette;
}

// Clair ou sombre pour cette application seulement (0 : celui du système).
// Sous Plasma, le jeu de couleurs KDE l'emporte sur l'indication de Qt : le
// style de Plasma (KColorScheme) suit alors le jeu Breeze clair ou sombre
// désigné par la propriété KDE_COLOR_SCHEME_PATH, avec la palette Qt
// correspondante. Hors de Plasma (jeu introuvable), seule l'indication de Qt
// sert (Qt 6.8 et plus).
void openheadset_set_color_scheme(int scheme)
{
#if QT_VERSION >= QT_VERSION_CHECK(6, 8, 0)
    Qt::ColorScheme value = Qt::ColorScheme::Unknown;
    if (scheme == 1) {
        value = Qt::ColorScheme::Light;
    } else if (scheme == 2) {
        value = Qt::ColorScheme::Dark;
    }
    QGuiApplication::styleHints()->setColorScheme(value);
#endif
    if (scheme == 0) {
        return;
    }
    const QString file = scheme == 1 ? QStringLiteral("color-schemes/BreezeLight.colors")
                                     : QStringLiteral("color-schemes/BreezeDark.colors");
    const QString path = QStandardPaths::locate(QStandardPaths::GenericDataLocation, file);
    if (path.isEmpty()) {
        return;
    }
    qApp->setProperty("KDE_COLOR_SCHEME_PATH", path);
    QGuiApplication::setPalette(palette_from_scheme(path));
}

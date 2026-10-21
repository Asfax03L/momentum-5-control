// Sous Wayland, KDE associe une fenêtre à son fichier .desktop par son
// identifiant, et le gestionnaire de tâches prend l'icône transmise par la
// fenêtre : sans icône explicite, Qt envoie « aucune icône » et la barre
// des tâches affiche une icône vide. On les fixe donc tous les deux.
#include "momentum-kirigami/src/identity.h"

#include <QGuiApplication>
#include <QIcon>
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

// Clair ou sombre pour cette application seulement (Qt 6.8 et plus) ; le
// thème Plasma choisit les couleurs correspondantes. Avant Qt 6.8, le
// choix est enregistré mais le thème du système reste appliqué.
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
#else
    (void)scheme;
#endif
}

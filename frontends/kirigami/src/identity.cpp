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
    QGuiApplication::setDesktopFileName(QStringLiteral("momentum-kirigami"));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("OpenHeadset Control"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.openheadset.control"),
                                                    QIcon::fromTheme(QStringLiteral("audio-headphones"))));
}

// Clair ou sombre pour cette application seulement (Qt 6.8 et plus) ; le
// thème Plasma choisit les couleurs correspondantes.
void openheadset_set_color_scheme(int scheme)
{
    Qt::ColorScheme value = Qt::ColorScheme::Unknown;
    if (scheme == 1) {
        value = Qt::ColorScheme::Light;
    } else if (scheme == 2) {
        value = Qt::ColorScheme::Dark;
    }
    QGuiApplication::styleHints()->setColorScheme(value);
}

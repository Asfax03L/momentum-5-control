// Sous Wayland, KDE associe une fenêtre à son fichier .desktop par son
// identifiant, et le gestionnaire de tâches prend l'icône transmise par la
// fenêtre : sans icône explicite, Qt envoie « aucune icône » et la barre
// des tâches affiche une icône vide. On les fixe donc tous les deux.
#include "momentum-kirigami/src/identity.h"

#include <QGuiApplication>
#include <QIcon>

void openheadset_set_identity()
{
    QGuiApplication::setDesktopFileName(QStringLiteral("momentum-kirigami"));
    QGuiApplication::setApplicationDisplayName(QStringLiteral("OpenHeadset Control"));
    QGuiApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.openheadset.control"),
                                                    QIcon::fromTheme(QStringLiteral("audio-headphones"))));
}

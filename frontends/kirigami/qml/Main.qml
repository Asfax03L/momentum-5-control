// OpenHeadset Control — client Kirigami (KDE Plasma).
//
// Composants natifs de KDE : la fenêtre suit le thème Plasma (couleurs,
// polices, clair ou sombre). Toutes les valeurs viennent de l'objet
// `Headset` (pont Rust) : un interrupteur garde l'état confirmé par le
// casque, et la barre de titre montre l'attente pendant l'envoi.

import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtQuick.Shapes
import org.kde.kirigami as Kirigami
import org.openheadset.control

Kirigami.ApplicationWindow {
    id: root
    title: headset.tr("OpenHeadset Control")
    width: Kirigami.Units.gridUnit * 48
    height: Kirigami.Units.gridUnit * 40
    minimumWidth: Kirigami.Units.gridUnit * 20
    minimumHeight: Kirigami.Units.gridUnit * 24
    // Montrée à la fin du chargement, sauf démarrage caché (`--cache`) avec
    // l'icône de la zone de notification.
    visible: false
    Component.onCompleted: {
        const pages = {
            "son": soundPage, "scenes": scenesPage, "batterie": batteryPage,
            "reglages": settingsPage, "apropos": aboutPage, "audio": audioPage
        }
        if (pages[headset.startPage])
            root.pageStack.replace(pages[headset.startPage])
        if (!(headset.startHidden && headset.trayAvailable)) {
            root.show()
        }
    }

    Headset {
        id: headset
        onShowRequested: {
            root.show()
            root.raise()
            root.requestActivate()
        }
        onPanelRequested: {
            if (quickPanel.visible) {
                quickPanel.hide()
            } else {
                quickPanel.show()
                quickPanel.raise()
                quickPanel.requestActivate()
            }
        }
        onQuitRequested: Qt.quit()
        onEqualizerRequested: {
            root.pageStack.replace(soundPage)
            root.show()
            root.raise()
            root.requestActivate()
        }
    }

    // Avec l'icône de la zone de notification, fermer la fenêtre la cache :
    // l'icône reste, et un clic dessus la rouvre.
    onClosing: (close) => {
        if (headset.trayAvailable) {
            close.accepted = false
            root.hide()
        }
    }

    // Panneau rapide : clic gauche sur l'icône de la zone de notification,
    // comme sous Windows. Contenu choisi dans la page Scènes (`panneau.txt`).
    // Il se ferme quand il perd le focus, ou par un second clic sur l'icône.
    Window {
        id: quickPanel
        objectName: "panneau-rapide"
        title: headset.tr("OpenHeadset Control")
        flags: Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
        width: Kirigami.Units.gridUnit * 22
        height: panelContent.implicitHeight + Kirigami.Units.largeSpacing * 2
        visible: false
        color: Kirigami.Theme.backgroundColor
        Kirigami.Theme.colorSet: Kirigami.Theme.Window
        Kirigami.Theme.inherit: false
        onActiveChanged: {
            if (!active) {
                hide()
            }
        }

        Rectangle {
            anchors.fill: parent
            color: "transparent"
            border.color: Kirigami.ColorUtils.linearInterpolation(
                Kirigami.Theme.backgroundColor, Kirigami.Theme.textColor, 0.2)
        }

        ColumnLayout {
            id: panelContent
            anchors.fill: parent
            anchors.margins: Kirigami.Units.largeSpacing
            spacing: Kirigami.Units.smallSpacing

            Kirigami.Heading {
                Layout.fillWidth: true
                text: headset.title
                level: 3
                elide: Text.ElideRight
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: headset.busy ? headset.tr("Demande en cours…") : headset.connected ? headset.batteryText : headset.connection
                wrapMode: Text.Wrap
                opacity: 0.8
            }
            Rectangle {
                visible: headset.battery >= 0
                Layout.fillWidth: true
                implicitHeight: Kirigami.Units.smallSpacing * 2
                radius: height / 2
                color: Kirigami.Theme.alternateBackgroundColor
                Rectangle {
                    width: parent.width * Math.max(0, headset.battery) / 100
                    height: parent.height
                    radius: parent.radius
                    color: headset.battery > 20 ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.negativeTextColor
                }
            }
            RowLayout {
                visible: headset.panelNoise && headset.connected
                Layout.fillWidth: true
                Repeater {
                    model: headset.noiseModes
                    QQC2.Button {
                        required property int index
                        required property string modelData
                        Layout.fillWidth: true
                        text: modelData
                        checkable: true
                        checked: headset.noiseMode === index
                        enabled: headset.writable
                        onClicked: {
                            headset.setNoiseModeIndex(index)
                            checked = Qt.binding(() => headset.noiseMode === index)
                        }
                    }
                }
            }
            RowLayout {
                visible: headset.panelLevel && headset.connected
                Layout.fillWidth: true
                QQC2.Label { text: headset.tr("ANC ↔ transparence") }
                QQC2.Slider {
                    Accessible.name: headset.tr("ANC ↔ transparence")
                    id: panelLevel
                    Layout.fillWidth: true
                    from: 0
                    to: 100
                    stepSize: 5
                    value: headset.level
                    enabled: headset.writable
                    onPressedChanged: {
                        if (!pressed) {
                            headset.sendLevel(Math.round(value))
                            value = Qt.binding(() => headset.level)
                        }
                    }
                }
                QQC2.Label {
                    text: Math.round(panelLevel.value) + " %"
                    Layout.minimumWidth: Kirigami.Units.gridUnit * 2.5
                }
            }
            RowLayout {
                visible: headset.connected
                Layout.fillWidth: true
                QQC2.Label { text: headset.tr("Anti-vent"); Layout.fillWidth: true }
                QQC2.Switch {
                    Accessible.name: headset.tr("Anti-vent")
                    id: panelAntiWind
                    checked: headset.antiWind
                    enabled: headset.writable
                    onToggled: {
                        headset.sendAntiWind(checked)
                        root.confirmed(panelAntiWind, () => headset.antiWind)
                    }
                }
            }
            RowLayout {
                visible: headset.panelProfile && headset.connected
                Layout.fillWidth: true
                QQC2.Label { text: headset.tr("Profil personnalisé"); Layout.fillWidth: true }
                QQC2.Switch {
                    Accessible.name: headset.tr("Profil personnalisé")
                    id: panelProfile
                    checked: headset.profileOn
                    enabled: headset.profileToggleEnabled
                    onToggled: {
                        headset.sendProfile(checked)
                        root.confirmed(panelProfile, () => headset.profileOn)
                    }
                }
            }
            Flow {
                visible: headset.panelScenes.length > 0 && headset.connected
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: headset.panelScenes
                    QQC2.Button {
                        required property string modelData
                        text: modelData
                        icon.name: "media-playback-start"
                        enabled: headset.writable && !headset.busy
                        onClicked: headset.applyScene(headset.sceneNames.indexOf(modelData))
                    }
                }
            }
            QQC2.Button {
                Layout.alignment: Qt.AlignRight
                text: headset.tr("Ouvrir OpenHeadset Control")
                icon.name: "window"
                flat: true
                onClicked: {
                    quickPanel.hide()
                    root.show()
                    root.raise()
                    root.requestActivate()
                }
            }
        }
    }

    // Essai de l'interface (CI, `--demo --essai-interface`) : chaque page se
    // charge, un réglage simulé change l'affichage, le panneau rapide
    // s'ouvre. Fin par « ESSAI INTERFACE RÉUSSI » (code 0) ou un échec
    // (code 1). Jamais actif hors du mode démonstration.
    property int essaiEtape: 0
    property bool essaiAntiVent: false
    property int essaiAttente: 0
    function essaiEchec(message) {
        console.error("ESSAI INTERFACE ÉCHEC : " + message)
        Qt.exit(1)
    }
    function essaiChercher(item, name) {
        if (!item)
            return null
        if (item.objectName === name)
            return item
        const enfants = item.children || []
        for (let i = 0; i < enfants.length; ++i) {
            const trouve = essaiChercher(enfants[i], name)
            if (trouve)
                return trouve
        }
        return null
    }
    Timer {
        running: headset.selfTest
        interval: 1500
        repeat: true
        onTriggered: {
            if (root.essaiEtape === 0) {
                const pages = [
                    [homePage, "page-accueil"], [soundPage, "page-egaliseur"],
                    [scenesPage, "page-scenes"], [batteryPage, "page-batterie"],
                    [settingsPage, "page-parametres"], [audioPage, "page-audio"],
                    [aboutPage, "page-apropos"]
                ]
                for (const [page, name] of pages) {
                    root.pageStack.replace(page)
                    const courante = root.pageStack.currentItem
                    if (!courante || courante.objectName !== name)
                        return root.essaiEchec("page " + name + " non chargée")
                    console.info("essai interface : " + name + " chargée")
                }
                root.pageStack.replace(homePage)
            } else if (root.essaiEtape === 1) {
                const interrupteur = root.essaiChercher(root.pageStack.currentItem, "accueil-anti-vent")
                if (!interrupteur)
                    return root.essaiEchec("interrupteur anti-vent introuvable")
                root.essaiAntiVent = headset.antiWind
                headset.sendAntiWind(!root.essaiAntiVent)
            } else if (root.essaiEtape === 2) {
                const interrupteur = root.essaiChercher(root.pageStack.currentItem, "accueil-anti-vent")
                if (headset.antiWind === root.essaiAntiVent || interrupteur.checked === root.essaiAntiVent) {
                    // Machine lente : jusqu'à cinq tops pour la relecture.
                    root.essaiAttente += 1
                    if (root.essaiAttente < 5)
                        return
                    return root.essaiEchec("anti-vent simulé non relu à l'affichage")
                }
                console.info("essai interface : anti-vent " + root.essaiAntiVent + " → " + headset.antiWind)
                headset.panelRequested()
            } else if (root.essaiEtape === 3) {
                if (!quickPanel.visible)
                    return root.essaiEchec("panneau rapide non ouvert")
                console.info("ESSAI INTERFACE RÉUSSI")
                Qt.exit(0)
            }
            root.essaiEtape += 1
        }
    }

    // Assistant de premier lancement : une étape à la fois (textes communs
    // avec WinUI, momentum-core/src/assistant.rs). Fermé = noté vu.
    Kirigami.Dialog {
        id: assistantDialog
        property int step: 0
        readonly property int count: headset.assistantCount()
        readonly property bool last: step === count - 1
        title: headset.assistantTitle(step)
        preferredWidth: Kirigami.Units.gridUnit * 30
        padding: Kirigami.Units.largeSpacing
        standardButtons: Kirigami.Dialog.NoButton
        onRejected: headset.finishAssistant()
        ColumnLayout {
            spacing: Kirigami.Units.largeSpacing
            QQC2.Label {
                Layout.fillWidth: true
                text: headset.tr("Premier lancement — étape {} sur {}")
                    .replace("{}", assistantDialog.step + 1).replace("{}", assistantDialog.count)
                font: Kirigami.Theme.smallFont
                opacity: 0.7
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: headset.assistantText(assistantDialog.step)
                wrapMode: Text.WordWrap
            }
            QQC2.Button {
                visible: headset.assistantBluetooth(assistantDialog.step)
                text: headset.tr("Ouvrir les paramètres Bluetooth")
                icon.name: "preferences-system-bluetooth"
                onClicked: headset.openBluetoothSettings()
            }
            QQC2.Label {
                Layout.fillWidth: true
                visible: headset.assistantShowsConnection(assistantDialog.step)
                text: headset.assistantConnectionText(headset.connected)
                font.bold: true
                wrapMode: Text.WordWrap
            }
        }
        customFooterActions: [
            Kirigami.Action {
                text: headset.tr("Précédent")
                icon.name: "go-previous"
                enabled: assistantDialog.step > 0
                onTriggered: assistantDialog.step -= 1
            },
            Kirigami.Action {
                text: assistantDialog.last ? headset.tr("Terminer") : headset.tr("Suivant")
                icon.name: assistantDialog.last ? "dialog-ok" : "go-next"
                onTriggered: {
                    if (assistantDialog.last) {
                        assistantDialog.close()
                        headset.finishAssistant()
                    } else {
                        assistantDialog.step += 1
                    }
                }
            },
            Kirigami.Action {
                text: headset.tr("Passer l'assistant")
                icon.name: "dialog-cancel"
                onTriggered: {
                    assistantDialog.close()
                    headset.finishAssistant()
                }
            }
        ]
    }
    Connections {
        target: headset
        function onAssistantOpenChanged() {
            if (headset.assistantOpen) {
                assistantDialog.step = 0
                assistantDialog.open()
            }
        }
    }
    Component.onCompleted: {
        if (headset.assistantOpen)
            assistantDialog.open()
    }

    // Essai de l'assistant (CI, `--demo --essai-assistant`) : chaque étape a
    // un titre et un texte, « Terminer » ferme. Fin par « ESSAI ASSISTANT
    // RÉUSSI » (code 0) ou un échec (code 1).
    property var essaiTitres: []
    Timer {
        running: headset.assistantTest
        interval: 800
        repeat: true
        onTriggered: {
            if (!assistantDialog.visible) {
                if (root.essaiTitres.length === assistantDialog.count && !headset.assistantOpen) {
                    console.info("ESSAI ASSISTANT RÉUSSI : " + root.essaiTitres.join(" | "))
                    Qt.exit(0)
                }
                console.error("ESSAI ASSISTANT ÉCHEC : fenêtre fermée trop tôt")
                Qt.exit(1)
            }
            const titre = headset.assistantTitle(assistantDialog.step)
            if (!titre || !headset.assistantText(assistantDialog.step)) {
                console.error("ESSAI ASSISTANT ÉCHEC : étape " + assistantDialog.step + " vide")
                Qt.exit(1)
            }
            root.essaiTitres = root.essaiTitres.concat([titre])
            console.info("essai assistant : étape " + (assistantDialog.step + 1) + " : " + titre)
            assistantDialog.customFooterActions[1].trigger()
        }
    }

    // Remet un interrupteur sur la valeur confirmée par le casque après un
    // clic : la nouvelle valeur n'apparaîtra qu'une fois relue.
    function confirmed(control, getter) {
        control.checked = Qt.binding(getter)
    }

    globalDrawer: Kirigami.GlobalDrawer {
        isMenu: !root.wideScreen
        modal: !root.wideScreen
        collapsible: true
        collapsed: !root.wideScreen
        showHeaderWhenCollapsed: true
        title: headset.tr("OpenHeadset Control")
        // Logo installé par scripts/installer-linux.sh (thème d'icônes).
        titleIcon: "org.openheadset.control"
        // Bouton « Close Sidebar » non traduit : le tiroir se replie déjà
        // tout seul en fenêtre étroite.
        collapseButtonVisible: false
        actions: [
            Kirigami.Action {
                text: headset.tr("Accueil")
                icon.name: "go-home"
                onTriggered: root.pageStack.replace(homePage)
            },
            Kirigami.Action {
                text: headset.tr("Égaliseur")
                icon.name: "view-media-equalizer"
                onTriggered: root.pageStack.replace(soundPage)
            },
            Kirigami.Action {
                text: headset.tr("Scènes")
                icon.name: "view-list-details"
                onTriggered: root.pageStack.replace(scenesPage)
            },
            Kirigami.Action {
                text: headset.tr("Batterie")
                icon.name: "battery-good"
                onTriggered: root.pageStack.replace(batteryPage)
            },
            Kirigami.Action {
                text: headset.tr("Paramètres")
                icon.name: "configure"
                onTriggered: root.pageStack.replace(settingsPage)
            },
            Kirigami.Action {
                text: headset.tr("Connexion et audio")
                icon.name: "audio-card"
                onTriggered: root.pageStack.replace(audioPage)
            },
            Kirigami.Action {
                text: headset.tr("À propos")
                icon.name: "help-about"
                onTriggered: root.pageStack.replace(aboutPage)
            }
        ]
    }

    pageStack.initialPage: homePage

    // En-tête commun : état de la connexion et envoi en cours.
    component StatusHeader: ColumnLayout {
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: headset.reconnectNotice !== ""
            text: headset.reconnectNotice
            type: Kirigami.MessageType.Information
        }
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: headset.banner !== ""
            text: headset.banner
            type: headset.bannerWarning ? Kirigami.MessageType.Warning : Kirigami.MessageType.Information
        }
        RowLayout {
            Layout.fillWidth: true
            QQC2.BusyIndicator {
                running: !headset.connected || headset.busy
                visible: running
                implicitWidth: Kirigami.Units.iconSizes.medium
                implicitHeight: implicitWidth
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: headset.busy ? headset.tr("Demande en cours…") : headset.connection
                elide: Text.ElideRight
            }
        }
    }

    // Accueil : casque, contrôle du bruit, fonctions rapides.
    Component {
        id: homePage
        Kirigami.ScrollablePage {
            objectName: "page-accueil"
            title: headset.tr("Accueil")
            actions: [
                Kirigami.Action {
                    displayComponent: QQC2.BusyIndicator {
                        running: headset.busy
                        visible: headset.busy
                    }
                }
            ]
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing

                StatusHeader {}

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Kirigami.Icon {
                            source: "audio-headphones"
                            implicitWidth: Kirigami.Units.iconSizes.enormous
                            implicitHeight: implicitWidth
                            opacity: headset.connected ? 1 : 0.5
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Kirigami.Heading {
                                text: headset.title
                                level: 1
                                Layout.fillWidth: true
                                wrapMode: Text.Wrap
                            }
                            QQC2.Label {
                                visible: headset.battery >= 0
                                text: headset.tr("Batterie : ") + headset.battery + " %"
                            }
                            // Jauge de batterie aux couleurs du thème Plasma.
                            Rectangle {
                                visible: headset.battery >= 0
                                Layout.fillWidth: true
                                implicitHeight: Kirigami.Units.smallSpacing * 2
                                radius: height / 2
                                color: Kirigami.Theme.alternateBackgroundColor
                                Rectangle {
                                    width: parent.width * Math.max(0, headset.battery) / 100
                                    height: parent.height
                                    radius: parent.radius
                                    color: headset.battery > 20 ? Kirigami.Theme.positiveTextColor : Kirigami.Theme.negativeTextColor
                                }
                            }
                            QQC2.Label {
                                visible: headset.firmware !== ""
                                text: headset.tr("Micrologiciel ") + headset.firmware + (headset.model !== "" ? " — " + headset.model : "")
                                opacity: 0.7
                            }
                            QQC2.Label {
                                visible: headset.stream !== ""
                                text: headset.stream
                                opacity: 0.7
                            }
                        }
                    }
                }

                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    visible: !headset.connected
                    icon.name: "network-bluetooth"
                    text: headset.tr("En attente du casque")
                    explanation: headset.waiting
                    helpfulAction: Kirigami.Action {
                        text: headset.tr("Reconnecter le casque")
                        icon.name: "network-bluetooth"
                        enabled: headset.canReconnect
                        onTriggered: headset.reconnectHeadset()
                    }
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: Kirigami.Heading {
                        text: headset.tr("Réduction de bruit")
                        level: 3
                    }
                    contentItem: ColumnLayout {
                        RowLayout {
                            Layout.fillWidth: true
                            Repeater {
                                model: headset.noiseModes
                                QQC2.Button {
                                    required property int index
                                    required property string modelData
                                    Layout.fillWidth: true
                                    text: modelData
                                    icon.name: index === 0 ? "audio-volume-high" : index === 1 ? "configure" : "audio-volume-muted"
                                    checkable: true
                                    checked: headset.noiseMode === index
                                    enabled: headset.writable
                                    onClicked: {
                                        headset.setNoiseModeIndex(index)
                                        checked = Qt.binding(() => headset.noiseMode === index)
                                    }
                                }
                            }
                        }
                        Kirigami.FormLayout {
                            Layout.fillWidth: true
                            RowLayout {
                                Kirigami.FormData.label: headset.tr("ANC ↔ transparence :")
                                QQC2.Slider {
                                    Accessible.name: headset.tr("ANC ↔ transparence")
                                    id: levelSlider
                                    Layout.fillWidth: true
                                    from: 0
                                    to: 100
                                    stepSize: 5
                                    value: headset.level
                                    enabled: headset.writable
                                    onPressedChanged: {
                                        if (!pressed) {
                                            headset.sendLevel(Math.round(value))
                                            value = Qt.binding(() => headset.level)
                                        }
                                    }
                                }
                                QQC2.Label {
                                    text: Math.round(levelSlider.value) + " %"
                                    Layout.minimumWidth: Kirigami.Units.gridUnit * 3
                                }
                            }
                            QQC2.Switch {
                                Accessible.name: headset.tr("Anti-vent")
                                id: antiWind
                                objectName: "accueil-anti-vent"
                                Kirigami.FormData.label: headset.tr("Anti-vent :")
                                checked: headset.antiWind
                                enabled: headset.writable
                                onToggled: {
                                    headset.sendAntiWind(checked)
                                    root.confirmed(antiWind, () => headset.antiWind)
                                }
                            }
                        }
                    }
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: Kirigami.Heading {
                        text: headset.tr("Fonctionnalités rapides")
                        level: 3
                    }
                    contentItem: Kirigami.FormLayout {
                        QQC2.Switch {
                            id: multipoint
                            Kirigami.FormData.label: headset.tr("Multipoint :")
                            text: headset.tr("Deux appareils à la fois")
                            checked: headset.multipoint
                            enabled: headset.multipointEnabled
                            onToggled: {
                                if (checked) {
                                    headset.sendMultipoint(true)
                                } else {
                                    multipointDialog.open()
                                }
                                root.confirmed(multipoint, () => headset.multipoint)
                            }
                        }
                        QQC2.Switch {
                            id: spatial
                            Kirigami.FormData.label: headset.tr("Audio spatial :")
                            text: headset.tr("Dolby Atmos")
                            checked: headset.spatial
                            enabled: headset.spatialEnabled
                            onToggled: {
                                headset.sendSpatial(checked)
                                root.confirmed(spatial, () => headset.spatial)
                            }
                        }
                        QQC2.Switch {
                            id: headTracking
                            Kirigami.FormData.label: headset.tr("Suivi de tête :")
                            text: headset.tr("Avec l'audio spatial")
                            checked: headset.headTracking
                            enabled: headset.headTrackingEnabled
                            onToggled: {
                                headset.sendHeadTracking(checked)
                                root.confirmed(headTracking, () => headset.headTracking)
                            }
                        }
                        QQC2.Switch {
                            Accessible.name: headset.tr("Commandes tactiles")
                            id: touch
                            Kirigami.FormData.label: headset.tr("Commandes tactiles :")
                            checked: headset.touch
                            enabled: headset.touchEnabled
                            onToggled: {
                                headset.sendTouch(checked)
                                root.confirmed(touch, () => headset.touch)
                            }
                        }
                        QQC2.Switch {
                            id: smartPause
                            Kirigami.FormData.label: headset.tr("Smart Pause :")
                            text: headset.tr("Pause au retrait du casque")
                            checked: headset.smartPause
                            enabled: headset.smartPauseEnabled
                            onToggled: {
                                headset.sendSmartPause(checked)
                                root.confirmed(smartPause, () => headset.smartPause)
                            }
                        }
                    }
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected && headset.connectionCount > 0
                    header: Kirigami.Heading {
                        text: headset.tr("Appareils appairés au casque")
                        level: 3
                    }
                    contentItem: ColumnLayout {
                        Repeater {
                            model: headset.connectionCount
                            RowLayout {
                                id: device
                                required property int index
                                // Relu à chaque nouvelle liste du casque.
                                readonly property int revision: headset.connectionRevision
                                Layout.fillWidth: true
                                Kirigami.Icon {
                                    source: "network-bluetooth"
                                    implicitWidth: Kirigami.Units.iconSizes.smallMedium
                                    implicitHeight: implicitWidth
                                }
                                ColumnLayout {
                                    Layout.fillWidth: true
                                    spacing: 0
                                    QQC2.Label {
                                        Layout.fillWidth: true
                                        text: (device.revision, headset.connectionName(device.index))
                                        elide: Text.ElideRight
                                    }
                                    QQC2.Label {
                                        text: (device.revision, headset.connectionStatus(device.index))
                                        font: Kirigami.Theme.smallFont
                                        opacity: 0.7
                                    }
                                }
                                QQC2.Button {
                                    text: (device.revision, headset.connectionAction(device.index))
                                    visible: (device.revision, headset.connectionCanAct(device.index))
                                    onClicked: {
                                        if (text === headset.tr("Déconnecter")) {
                                            disconnectDialog.deviceIndex = device.index
                                            disconnectDialog.deviceName = headset.connectionName(device.index)
                                            disconnectDialog.open()
                                        } else {
                                            headset.toggleConnection(device.index)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Kirigami.PromptDialog {
                id: disconnectDialog
                property int deviceIndex: -1
                property string deviceName: ""
                title: headset.tr("Déconnecter « ") + deviceName + " » ?"
                subtitle: headset.tr("Le casque coupe sa liaison avec cet appareil. Il reste appairé : « Connecter » le rétablit.")
                standardButtons: Kirigami.Dialog.NoButton
                customFooterActions: [
                    Kirigami.Action {
                        text: headset.tr("Déconnecter")
                        icon.name: "network-disconnect"
                        onTriggered: {
                            headset.toggleConnection(disconnectDialog.deviceIndex)
                            disconnectDialog.close()
                        }
                    },
                    Kirigami.Action {
                        text: headset.tr("Annuler")
                        icon.name: "dialog-cancel"
                        onTriggered: disconnectDialog.close()
                    }
                ]
            }

            Kirigami.PromptDialog {
                id: multipointDialog
                title: headset.tr("Couper le Multipoint ?")
                subtitle: headset.tr("Sans Multipoint, le casque ne garde qu'un appareil : l'autre est déconnecté.")
                standardButtons: Kirigami.Dialog.NoButton
                customFooterActions: [
                    Kirigami.Action {
                        text: headset.tr("Couper le Multipoint")
                        icon.name: "dialog-ok"
                        onTriggered: {
                            headset.sendMultipoint(false)
                            multipointDialog.close()
                        }
                    },
                    Kirigami.Action {
                        text: headset.tr("Annuler")
                        icon.name: "dialog-cancel"
                        onTriggered: multipointDialog.close()
                    }
                ]
            }
        }
    }

    // Égaliseur : mode audio, bandes, préréglages.
    Component {
        id: soundPage
        Kirigami.ScrollablePage {
            objectName: "page-egaliseur"
            title: headset.tr("Égaliseur")
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing

                StatusHeader {}

                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    visible: !headset.connected
                    icon.name: "view-media-equalizer"
                    text: headset.tr("Égaliseur indisponible")
                    explanation: headset.waiting
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: Kirigami.Heading {
                        text: headset.tr("Mode audio")
                        level: 3
                    }
                    contentItem: RowLayout {
                        Repeater {
                            model: headset.audioModes
                            QQC2.Button {
                                required property int index
                                required property string modelData
                                Layout.fillWidth: true
                                text: modelData
                                checkable: true
                                checked: headset.audioMode === index
                                enabled: headset.modesEnabled && (index !== 2 || headset.profileAvailable)
                                onClicked: {
                                    headset.sendAudioModeIndex(index)
                                    checked = Qt.binding(() => headset.audioMode === index)
                                }
                            }
                        }
                    }
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: RowLayout {
                        Kirigami.Heading {
                            Layout.fillWidth: true
                            text: headset.tr("Égaliseur graphique — ") + headset.bandCount + headset.tr(" bandes")
                            level: 3
                        }
                        QQC2.Button {
                            text: headset.tr("Réinitialiser")
                            icon.name: "edit-reset"
                            enabled: headset.writable
                            onClicked: headset.sendFlat()
                        }
                    }
                    contentItem: ColumnLayout {
                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: Kirigami.Units.gridUnit * 14
                            Repeater {
                                model: headset.bandCount
                                ColumnLayout {
                                    id: band
                                    required property int index
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                    QQC2.Label {
                                        Layout.alignment: Qt.AlignHCenter
                                        text: (bandSlider.value > 0 ? "+" : "") + bandSlider.value.toLocaleString(Qt.locale(), "f", 1)
                                        font: Kirigami.Theme.smallFont
                                    }
                                    QQC2.Slider {
                                        Accessible.name: headset.tr("Bande {}").replace("{}", headset.bandLabel(band.index))
                                        id: bandSlider
                                        Layout.alignment: Qt.AlignHCenter
                                        Layout.fillHeight: true
                                        orientation: Qt.Vertical
                                        from: headset.minGain
                                        to: headset.maxGain
                                        stepSize: 0.5
                                        enabled: headset.writable
                                        // Relu à chaque nouvelle valeur du casque.
                                        value: {
                                            headset.eqRevision
                                            return headset.bandGain(band.index)
                                        }
                                        onPressedChanged: {
                                            if (!pressed) {
                                                headset.sendBand(band.index, value)
                                                value = Qt.binding(() => {
                                                    headset.eqRevision
                                                    return headset.bandGain(band.index)
                                                })
                                            }
                                        }
                                    }
                                    QQC2.Label {
                                        Layout.alignment: Qt.AlignHCenter
                                        text: {
                                            headset.eqRevision
                                            return headset.bandLabel(band.index)
                                        }
                                        font: Kirigami.Theme.smallFont
                                        wrapMode: Text.WordWrap
                                        horizontalAlignment: Text.AlignHCenter
                                        Layout.maximumWidth: Kirigami.Units.gridUnit * 3
                                    }
                                }
                            }
                        }
                        QQC2.Switch {
                            id: bass
                            text: headset.tr("Amplification des basses")
                            checked: headset.bassBoost
                            enabled: headset.writable
                            onToggled: {
                                headset.sendBassBoost(checked)
                                root.confirmed(bass, () => headset.bassBoost)
                            }
                        }
                    }
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: Kirigami.Heading {
                        text: headset.tr("Préréglages")
                        level: 3
                    }
                    contentItem: Flow {
                        spacing: Kirigami.Units.smallSpacing
                        Repeater {
                            model: headset.presetNames
                            QQC2.Button {
                                required property int index
                                required property string modelData
                                text: modelData
                                checkable: true
                                checked: headset.presetIndex === index
                                enabled: headset.writable
                                onClicked: {
                                    headset.applyPresetIndex(index)
                                    checked = Qt.binding(() => headset.presetIndex === index)
                                }
                            }
                        }
                    }
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: Kirigami.Heading {
                        text: headset.tr("Préréglages et courbes")
                        level: 3
                    }
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        RowLayout {
                            QQC2.TextField {
                                id: presetName
                                Layout.fillWidth: true
                                placeholderText: headset.tr("Nom du préréglage")
                                enabled: headset.writable
                            }
                            QQC2.Button {
                                text: headset.tr("Enregistrer la courbe")
                                enabled: headset.writable && presetName.text.trim() !== ""
                                onClicked: {
                                    if (headset.savePreset(presetName.text))
                                        presetName.text = ""
                                }
                            }
                        }
                        QQC2.Button {
                            text: headset.userPreset === ""
                                ? headset.tr("Supprimer le préréglage (choisir un préréglage personnel)")
                                : headset.tr("Supprimer « ") + headset.userPreset + " »"
                            enabled: headset.userPreset !== ""
                            onClicked: headset.deleteUserPreset()
                        }
                        RowLayout {
                            QQC2.TextField {
                                id: gainsText
                                Layout.fillWidth: true
                                placeholderText: headset.tr("Gains en dB : -3,5 ; -1 ; 0 ; …")
                                enabled: headset.writable
                            }
                            QQC2.Button {
                                text: headset.tr("Appliquer")
                                enabled: headset.writable && gainsText.text.trim() !== ""
                                onClicked: {
                                    if (headset.applyGainsText(gainsText.text))
                                        gainsText.text = ""
                                }
                            }
                        }
                        RowLayout {
                            QQC2.Button {
                                text: headset.tr("Exporter la courbe")
                                onClicked: headset.exportCurve()
                            }
                            QQC2.Button {
                                text: headset.tr("Importer une courbe…")
                                enabled: headset.writable
                                onClicked: curveDialog.open()
                            }
                        }
                        RowLayout {
                            QQC2.Button {
                                text: headset.tr("Mémoriser A")
                                enabled: headset.writable
                                onClicked: headset.rememberA()
                            }
                            QQC2.Button {
                                text: headset.abLabel
                                enabled: headset.writable && headset.abReady
                                onClicked: headset.toggleAb()
                            }
                        }
                        QQC2.Button {
                            text: headset.tr("Exporter le profil auditif")
                            enabled: headset.profileAvailable
                            onClicked: headset.exportProfile()
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            opacity: 0.7
                            text: headset.eqNotice !== "" ? headset.eqNotice
                                : headset.tr("Préréglages enregistrés sur ce PC (prereglages.txt). Courbe exportée dans Documents ; une courbe d'un autre modèle ou d'un autre nombre de bandes est refusée. Le profil auditif (lecture seule, aucune restauration) est une donnée personnelle : ne pas le publier.")
                        }
                        FileDialog {
                            id: curveDialog
                            title: headset.tr("Importer une courbe d'égaliseur")
                            nameFilters: [headset.tr("Courbes d'égaliseur (*.txt)")]
                            onAccepted: headset.importCurve(selectedFile.toString())
                        }
                    }
                }
            }
        }
    }

    // Paramètres du casque.
    Component {
        id: settingsPage
        Kirigami.ScrollablePage {
            objectName: "page-parametres"
            title: headset.tr("Paramètres")
            ColumnLayout {
                StatusHeader {}
                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        QQC2.Label {
                            text: headset.tr("Sauvegarder et retrouver mes réglages")
                            font.bold: true
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            text: headset.tr("Scènes, préréglages, raccourcis, panneau et apparence. Aucun profil auditif. Relancer l'interface après import ; un brouillon non enregistré n'est pas inclus.")
                        }
                        QQC2.CheckBox {
                            id: settingsBattery
                            text: headset.tr("Inclure aussi l'historique de batterie (export et import)")
                            onToggled: headset.cancelSettings()
                        }
                        QQC2.Button {
                            text: headset.tr("Exporter les réglages…")
                            onClicked: { headset.cancelSettings(); settingsSave.open() }
                        }
                        QQC2.Button {
                            text: headset.tr("Choisir un fichier à importer…")
                            onClicked: { headset.cancelSettings(); settingsOpen.open() }
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            text: headset.settingsNotice
                            textFormat: Text.PlainText
                        }
                        QQC2.Button {
                            text: headset.tr("Confirmer cet import")
                            enabled: headset.settingsReady
                            onClicked: headset.confirmSettings()
                        }
                        QQC2.Button {
                            text: headset.tr("Annuler l'import")
                            enabled: headset.settingsReady
                            onClicked: headset.cancelSettings()
                        }
                    }
                }
                FileDialog {
                    id: settingsSave
                    title: headset.tr("Exporter vers un nouveau fichier")
                    fileMode: FileDialog.SaveFile
                    nameFilters: [headset.tr("Réglages JSON (*.json)")]
                    defaultSuffix: "json"
                    onAccepted: headset.exportSettings(selectedFile.toString(), settingsBattery.checked)
                }
                FileDialog {
                    id: settingsOpen
                    title: headset.tr("Choisir les réglages à examiner")
                    fileMode: FileDialog.OpenFile
                    nameFilters: [headset.tr("Réglages JSON (*.json)")]
                    onAccepted: headset.previewSettings(selectedFile.toString(), settingsBattery.checked)
                }
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: headset.tr("Application")
                    }
                    QQC2.Switch {
                        id: autostartSwitch
                        Kirigami.FormData.label: headset.tr("Ouverture de session :")
                        text: headset.tr("Lancer OpenHeadset Control (icône seule)")
                        checked: headset.autostart
                        onToggled: {
                            headset.setAutostartEnabled(checked)
                            root.confirmed(autostartSwitch, () => headset.autostart)
                        }
                    }
                    RowLayout {
                        Kirigami.FormData.label: headset.tr("Langue :")
                        Repeater {
                            // Chaque langue dans sa propre langue.
                            model: [headset.tr("Langue du système"), "Français", "English"]
                            QQC2.RadioButton {
                                required property int index
                                required property string modelData
                                text: modelData
                                checked: headset.languageChoice === index
                                onClicked: headset.chooseLanguage(index)
                            }
                        }
                    }
                    QQC2.Label {
                        visible: headset.languageNotice !== ""
                        text: headset.languageNotice
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    RowLayout {
                        Kirigami.FormData.label: headset.tr("Thème :")
                        QQC2.RadioButton {
                            text: headset.tr("Clair")
                            checked: headset.themeChoice === 0
                            onClicked: headset.chooseTheme(0)
                        }
                        QQC2.RadioButton {
                            text: headset.tr("Sombre")
                            checked: headset.themeChoice === 1
                            onClicked: headset.chooseTheme(1)
                        }
                        QQC2.Label {
                            visible: headset.themeChoice < 0
                            text: headset.tr("(pour l'instant celui de Plasma)")
                            opacity: 0.7
                        }
                    }
                    QQC2.Button {
                        Kirigami.FormData.label: headset.tr("Raccourcis clavier :")
                        text: headset.tr("Modifier dans KDE…")
                        icon.name: "preferences-desktop-keyboard-shortcut"
                        // Flatpak : Configuration du système > Raccourcis.
                        visible: !headset.sandboxed
                        onClicked: headset.openShortcutSettings()
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        text: headset.tr("Rubrique « OpenHeadset Control » : mode de bruit suivant (Ctrl+Alt+N), profil personnalisé (Ctrl+Alt+P), fenêtre (Ctrl+Alt+M), et une action par scène, sans raccourci au départ.")
                    }
                }
                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    visible: !headset.connected
                    icon.name: "configure"
                    text: headset.tr("Réglages indisponibles")
                    explanation: headset.waiting
                }
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    visible: headset.connected

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: headset.tr("Appels")
                    }
                    QQC2.Switch {
                        Accessible.name: headset.tr("Réponse automatique")
                        id: autoAnswer
                        Kirigami.FormData.label: headset.tr("Réponse automatique :")
                        checked: headset.autoAnswer
                        enabled: headset.autoAnswerEnabled
                        onToggled: {
                            headset.sendAutoAnswer(checked)
                            root.confirmed(autoAnswer, () => headset.autoAnswer)
                        }
                    }
                    QQC2.Switch {
                        Accessible.name: headset.tr("Appels confortables")
                        id: comfort
                        Kirigami.FormData.label: headset.tr("Appels confortables :")
                        checked: headset.comfort
                        enabled: headset.comfortEnabled
                        onToggled: {
                            headset.sendComfort(checked)
                            root.confirmed(comfort, () => headset.comfort)
                        }
                    }

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: headset.tr("Alimentation")
                    }
                    QQC2.ComboBox {
                        Accessible.name: headset.tr("Extinction automatique")
                        Kirigami.FormData.label: headset.tr("Extinction automatique :")
                        model: headset.autoOffLabels
                        currentIndex: headset.autoOff
                        enabled: headset.autoOffEnabled
                        onActivated: index => {
                            headset.sendAutoOffIndex(index)
                            currentIndex = Qt.binding(() => headset.autoOff)
                        }
                    }
                    QQC2.Switch {
                        Accessible.name: headset.tr("Protection de la batterie")
                        id: protection
                        Kirigami.FormData.label: headset.tr("Protection de la batterie :")
                        checked: headset.batteryProtection
                        enabled: headset.batteryProtectionEnabled
                        onToggled: {
                            headset.sendBatteryProtection(checked)
                            root.confirmed(protection, () => headset.batteryProtection)
                        }
                    }

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: headset.tr("Invites")
                    }
                    QQC2.ComboBox {
                        Accessible.name: headset.tr("Invites")
                        Kirigami.FormData.label: headset.tr("Invites :")
                        model: headset.promptModes
                        currentIndex: headset.promptMode
                        enabled: headset.promptsEnabled
                        onActivated: index => {
                            headset.sendPromptModeIndex(index)
                            currentIndex = Qt.binding(() => headset.promptMode)
                        }
                    }
                    RowLayout {
                        Kirigami.FormData.label: headset.tr("Volume des invites :")
                        QQC2.Slider {
                            Accessible.name: headset.tr("Volume des invites")
                            id: promptVolume
                            Layout.fillWidth: true
                            from: 0
                            to: 100
                            stepSize: 1
                            value: Math.max(0, headset.promptVolume)
                            enabled: headset.promptVolumeEnabled
                            onPressedChanged: {
                                if (!pressed) {
                                    headset.sendPromptVolume(Math.round(value))
                                    value = Qt.binding(() => Math.max(0, headset.promptVolume))
                                }
                            }
                        }
                        QQC2.Label {
                            text: headset.promptVolume < 0 ? headset.tr("non lu") : Math.round(promptVolume.value) + " %"
                            Layout.minimumWidth: Kirigami.Units.gridUnit * 3
                        }
                    }

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: headset.tr("Nom du casque")
                    }
                    RowLayout {
                        Kirigami.FormData.label: headset.tr("Nouveau nom :")
                        QQC2.TextField {
                            id: newName
                            Layout.fillWidth: true
                            placeholderText: headset.headsetName === "" ? headset.tr("non lu") : headset.headsetName
                            enabled: headset.renameEnabled
                            maximumLength: 32
                            onTextEdited: nameProblem.text = ""
                        }
                        QQC2.Button {
                            text: headset.tr("Renommer")
                            enabled: headset.renameEnabled && newName.text.trim() !== ""
                            onClicked: {
                                const problem = headset.rename(newName.text)
                                nameProblem.text = problem
                                if (problem === "")
                                    newName.text = ""
                            }
                        }
                    }
                    QQC2.Label {
                        id: nameProblem
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: Kirigami.Theme.negativeTextColor
                        visible: text !== ""
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        text: headset.tr("Lettres sans accent, chiffres et espaces, 32 caractères au plus. Le système et le téléphone peuvent garder l'ancien nom jusqu'à la prochaine connexion.")
                    }
                }
            }
        }
    }

    // Scènes : plusieurs réglages validés appliqués d'un coup.
    Component {
        id: scenesPage
        Kirigami.ScrollablePage {
            objectName: "page-scenes"
            title: headset.tr("Scènes")
            Component.onCompleted: headset.reloadScenes()
            actions: [
                Kirigami.Action {
                    text: headset.tr("Modifier scenes.txt")
                    icon.name: "document-edit"
                    onTriggered: Qt.openUrlExternally(headset.scenesUrl)
                },
                Kirigami.Action {
                    text: headset.tr("Relire")
                    icon.name: "view-refresh"
                    onTriggered: headset.reloadScenes()
                }
            ]
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                StatusHeader {}
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: headset.sceneProblems !== ""
                    type: Kirigami.MessageType.Warning
                    text: headset.sceneProblems
                }
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: headset.sceneResult !== ""
                    type: headset.sceneResultOk ? Kirigami.MessageType.Positive : Kirigami.MessageType.Warning
                    text: headset.sceneResult
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.Wrap
                    opacity: 0.7
                    text: headset.tr("Chaque réglage d'une scène est appliqué puis relu sur le casque, un par un. Une étape refusée arrête la scène et l'affiche ici.")
                }
                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: headset.tr("Enregistrer les réglages déjà lus. Les valeurs non lues restent absentes ; aucun réglage du casque ne change.")
                        }
                        QQC2.TextField {
                            id: captureSceneName
                            Layout.fillWidth: true
                            placeholderText: headset.tr("Nom de la scène")
                            Accessible.name: headset.tr("Nom de la scène à enregistrer")
                            maximumLength: 32
                        }
                        QQC2.CheckBox {
                            id: captureSceneReplace
                            text: headset.tr("Remplacer la scène et son préréglage existants")
                        }
                        QQC2.Button {
                            text: headset.tr("Enregistrer l'état actuel comme scène")
                            icon.name: "document-save"
                            enabled: headset.connected && !headset.busy && captureSceneName.text.trim() !== ""
                            onClicked: headset.saveCurrentScene(captureSceneName.text, captureSceneReplace.checked)
                        }
                    }
                }
                Repeater {
                    model: headset.sceneNames
                    Kirigami.AbstractCard {
                        required property int index
                        required property string modelData
                        Layout.fillWidth: true
                        contentItem: RowLayout {
                            ColumnLayout {
                                Layout.fillWidth: true
                                Kirigami.Heading {
                                    text: modelData
                                    level: 3
                                }
                                QQC2.Label {
                                    Layout.fillWidth: true
                                    text: headset.scenePreviews[index] ?? ""
                                    wrapMode: Text.Wrap
                                    opacity: 0.7
                                }
                            }
                            QQC2.Button {
                                text: headset.tr("Appliquer")
                                icon.name: "media-playback-start"
                                enabled: headset.writable && !headset.busy
                                onClicked: headset.applyScene(index)
                            }
                        }
                    }
                }

                // Contenu du panneau rapide (clic sur l'icône), `panneau.txt`.
                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    header: Kirigami.Heading {
                        text: headset.tr("Panneau rapide")
                        level: 3
                    }
                    contentItem: ColumnLayout {
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            opacity: 0.7
                            text: headset.tr("Ouvert par un clic sur l'icône de la zone de notification : batterie et anti-vent, plus ce qui est coché ici (quatre scènes au plus).")
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.largeSpacing
                            QQC2.CheckBox {
                                id: panelNoiseBox
                                text: headset.tr("Contrôle du bruit")
                                checked: headset.panelNoise
                                onToggled: {
                                    headset.setPanelOption("bruit", checked)
                                    root.confirmed(panelNoiseBox, () => headset.panelNoise)
                                }
                            }
                            QQC2.CheckBox {
                                id: panelLevelBox
                                text: headset.tr("Curseur ANC ↔ transparence")
                                checked: headset.panelLevel
                                onToggled: {
                                    headset.setPanelOption("niveau", checked)
                                    root.confirmed(panelLevelBox, () => headset.panelLevel)
                                }
                            }
                            QQC2.CheckBox {
                                id: panelProfileBox
                                text: headset.tr("Profil personnalisé")
                                checked: headset.panelProfile
                                onToggled: {
                                    headset.setPanelOption("profil", checked)
                                    root.confirmed(panelProfileBox, () => headset.panelProfile)
                                }
                            }
                        }
                        Flow {
                            Layout.fillWidth: true
                            visible: headset.sceneNames.length > 0
                            spacing: Kirigami.Units.largeSpacing
                            QQC2.Label { text: headset.tr("Scènes :") }
                            Repeater {
                                model: headset.sceneNames
                                QQC2.CheckBox {
                                    id: panelSceneBox
                                    required property string modelData
                                    text: modelData
                                    checked: headset.panelScenes.indexOf(modelData) >= 0
                                    onToggled: {
                                        headset.setPanelScene(modelData, checked)
                                        root.confirmed(panelSceneBox, () => headset.panelScenes.indexOf(modelData) >= 0)
                                    }
                                }
                            }
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            visible: headset.panelNotice !== ""
                            text: headset.panelNotice
                            wrapMode: Text.Wrap
                            opacity: 0.7
                        }
                    }
                }

                // Éditeur : le brouillon n'est écrit qu'avec « Enregistrer ».
                Kirigami.AbstractCard {
                    id: sceneEditor
                    Layout.fillWidth: true
                    function send() {
                        headset.setSceneForm(sceneName.text, sceneLevel.text, [sceneSound.currentIndex,
                            sceneSpatial.currentIndex, sceneNoise.currentIndex,
                            sceneAntiWind.currentIndex, sceneTouch.currentIndex,
                            sceneEq.currentIndex, sceneBass.currentIndex].join(","))
                    }
                    header: Kirigami.Heading {
                        text: headset.tr("Modifier les scènes")
                        level: 3
                    }
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Repeater {
                                model: headset.sceneEditNames
                                QQC2.Button {
                                    required property int index
                                    required property string modelData
                                    text: modelData
                                    checkable: true
                                    checked: headset.sceneEditSelected === index
                                    onClicked: {
                                        headset.sceneEditSelect(index)
                                        checked = Qt.binding(() => headset.sceneEditSelected === index)
                                    }
                                }
                            }
                        }
                        RowLayout {
                            QQC2.Button { text: headset.tr("Nouvelle"); icon.name: "list-add"; onClicked: headset.sceneEditAction("nouvelle") }
                            QQC2.Button { text: headset.tr("Dupliquer"); icon.name: "edit-copy"; enabled: headset.sceneEditNames.length > 0; onClicked: headset.sceneEditAction("dupliquer") }
                            QQC2.Button { text: headset.tr("Supprimer"); icon.name: "edit-delete"; enabled: headset.sceneEditNames.length > 0; onClicked: headset.sceneEditAction("supprimer") }
                            QQC2.Button { text: headset.tr("Monter"); icon.name: "go-up"; enabled: headset.sceneEditSelected > 0; onClicked: headset.sceneEditAction("monter") }
                            QQC2.Button { text: headset.tr("Descendre"); icon.name: "go-down"; enabled: headset.sceneEditSelected + 1 < headset.sceneEditNames.length; onClicked: headset.sceneEditAction("descendre") }
                        }
                        Kirigami.FormLayout {
                            Layout.fillWidth: true
                            visible: headset.sceneEditNames.length > 0
                            QQC2.TextField {
                                Accessible.name: headset.tr("Nom de la scène")
                                id: sceneName
                                Kirigami.FormData.label: headset.tr("Nom :")
                                text: headset.sceneFormName
                                onTextEdited: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                Accessible.name: headset.tr("Mode audio")
                                id: sceneSound
                                Kirigami.FormData.label: headset.tr("Mode audio :")
                                model: headset.sceneSoundChoices
                                currentIndex: headset.sceneFormSound
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                Accessible.name: headset.tr("Audio spatial")
                                id: sceneSpatial
                                Kirigami.FormData.label: headset.tr("Audio spatial :")
                                model: headset.sceneSwitchChoices
                                currentIndex: headset.sceneFormSpatial
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                Accessible.name: headset.tr("Contrôle du bruit")
                                id: sceneNoise
                                Kirigami.FormData.label: headset.tr("Contrôle du bruit :")
                                model: headset.sceneNoiseChoices
                                currentIndex: headset.sceneFormNoise
                                onActivated: sceneEditor.send()
                            }
                            QQC2.TextField {
                                id: sceneLevel
                                Kirigami.FormData.label: headset.tr("Niveau (0 à 100) :")
                                placeholderText: headset.tr("vide = inchangé")
                                text: headset.sceneFormLevel
                                onTextEdited: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                Accessible.name: headset.tr("Anti-vent")
                                id: sceneAntiWind
                                Kirigami.FormData.label: headset.tr("Anti-vent :")
                                model: headset.sceneSwitchChoices
                                currentIndex: headset.sceneFormAntiWind
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                Accessible.name: headset.tr("Commandes tactiles")
                                id: sceneTouch
                                Kirigami.FormData.label: headset.tr("Commandes tactiles :")
                                model: headset.sceneSwitchChoices
                                currentIndex: headset.sceneFormTouch
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                Accessible.name: headset.tr("Préréglage d'égaliseur")
                                id: sceneEq
                                Kirigami.FormData.label: headset.tr("Préréglage d'égaliseur :")
                                model: headset.sceneEqChoices
                                currentIndex: headset.sceneFormEqualizer
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                Accessible.name: headset.tr("Amplification des basses")
                                id: sceneBass
                                Kirigami.FormData.label: headset.tr("Amplification des basses :")
                                model: headset.sceneSwitchChoices
                                currentIndex: headset.sceneFormBass
                                onActivated: sceneEditor.send()
                            }
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: headset.sceneEditPreview
                            opacity: 0.8
                        }
                        RowLayout {
                            QQC2.Button {
                                text: headset.tr("Enregistrer")
                                icon.name: "document-save"
                                enabled: headset.sceneEditCanSave
                                onClicked: headset.sceneEditAction("enregistrer")
                            }
                            QQC2.Button {
                                text: headset.tr("Annuler les modifications")
                                icon.name: "edit-undo"
                                enabled: headset.sceneEditDirty
                                onClicked: headset.sceneEditAction("annuler")
                            }
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: headset.sceneEditNotice
                            opacity: 0.8
                        }
                    }
                }
            }
        }
    }

    // Connexion et audio : chaque valeur avec sa source et l'heure de lecture.
    component AudioSection: ColumnLayout {
        required property string title
        required property string section
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing
        Kirigami.Heading {
            text: parent.title
            level: 3
        }
        Repeater {
            model: {
                headset.audioRevision
                return headset.audioRows(parent.section)
            }
            ColumnLayout {
                required property string modelData
                Layout.fillWidth: true
                spacing: 0
                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    text: modelData.split("\t")[0]
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    visible: modelData.indexOf("\t") >= 0
                    text: modelData.split("\t")[1] || ""
                    opacity: 0.7
                    font: Kirigami.Theme.smallFont
                }
            }
        }
    }
    Component {
        id: audioPage
        Kirigami.ScrollablePage {
            objectName: "page-audio"
            id: audioScroll
            title: headset.tr("Connexion et audio")
            Component.onCompleted: headset.reloadAudio()
            actions: [Kirigami.Action {
                text: headset.audioReading ? headset.tr("Lecture PipeWire…") : headset.tr("Relire PipeWire")
                icon.name: "view-refresh"
                enabled: !headset.audioReading
                onTriggered: headset.reloadAudio()
            }]
            Timer {
                interval: 15000
                running: audioScroll.visible
                repeat: true
                onTriggered: headset.reloadAudio()
            }
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                StatusHeader {}
                AudioSection { title: headset.tr("Constats"); section: "constats" }
                AudioSection { title: headset.tr("Pilotage du casque"); section: "pilotage" }
                AudioSection { title: headset.tr("Sortie audio du système"); section: "systeme" }
                AudioSection { title: headset.tr("Flux Bluetooth"); section: "flux" }
            }
        }
    }

    // Batterie : courbe enregistrée par le service, sur ce PC.
    Component {
        id: batteryPage
        Kirigami.ScrollablePage {
            objectName: "page-batterie"
            id: batteryScroll
            title: headset.tr("Batterie")
            property string notice: ""
            Component.onCompleted: headset.reloadBattery(headset.batterySpan)
            actions: [
                Kirigami.Action {
                    text: headset.tr("Exporter (CSV)")
                    icon.name: "document-export"
                    onTriggered: batteryScroll.notice = headset.exportBattery()
                },
                Kirigami.Action {
                    text: headset.tr("Effacer l'historique…")
                    icon.name: "edit-clear-history"
                    onTriggered: clearBatteryDialog.open()
                }
            ]
            Kirigami.PromptDialog {
                id: clearBatteryDialog
                title: headset.tr("Effacer l'historique de la batterie ?")
                subtitle: headset.tr("Toutes les mesures enregistrées sur ce PC sont supprimées. Exporter d'abord pour les garder.")
                standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel
                onAccepted: batteryScroll.notice = headset.clearBattery()
            }
            Timer {
                interval: 60000
                running: true
                repeat: true
                onTriggered: headset.reloadBattery(headset.batterySpan)
            }
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing
                StatusHeader {}
                Kirigami.InlineMessage {
                    Layout.fillWidth: true
                    visible: batteryScroll.notice !== ""
                    text: batteryScroll.notice
                    showCloseButton: true
                    onVisibleChanged: if (!visible) batteryScroll.notice = ""
                }
                RowLayout {
                    Kirigami.Heading {
                        Layout.fillWidth: true
                        text: headset.tr("Batterie : ") + headset.batteryCurrent
                        level: 2
                    }
                    QQC2.ComboBox {
                        Accessible.name: headset.tr("Période de l'historique")
                        model: headset.batterySpans
                        currentIndex: headset.batterySpan
                        onActivated: index => headset.reloadBattery(index)
                    }
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 10
                    color: Kirigami.Theme.alternateBackgroundColor
                    radius: Kirigami.Units.cornerRadius
                    Shape {
                        id: chart
                        anchors.fill: parent
                        anchors.margins: Kirigami.Units.smallSpacing
                        ShapePath {
                            strokeColor: Kirigami.Theme.highlightColor
                            strokeWidth: 2
                            fillColor: "transparent"
                            joinStyle: ShapePath.RoundJoin
                            PathSvg {
                                path: {
                                    headset.batteryRevision
                                    return headset.batteryChart(chart.width, chart.height)
                                }
                            }
                        }
                    }
                    QQC2.Label {
                        anchors.centerIn: parent
                        visible: headset.batteryChart(1000, 100) === ""
                        text: headset.tr("Aucune mesure sur la période")
                        opacity: 0.7
                    }
                }
                RowLayout {
                    Layout.fillWidth: true
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: headset.batteryAxis
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                    }
                    QQC2.Label {
                        text: headset.tr("maintenant")
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                    }
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: headset.batteryEstimate
                    wrapMode: Text.Wrap
                }
                Repeater {
                    model: headset.batteryDetails
                    QQC2.Label {
                        required property string modelData
                        Layout.fillWidth: true
                        text: modelData
                        wrapMode: Text.Wrap
                        opacity: 0.7
                    }
                }
            }
        }
    }

    // À propos.
    Component {
        id: aboutPage
        Kirigami.ScrollablePage {
            objectName: "page-apropos"
            title: headset.tr("À propos")
            actions: [
                Kirigami.Action {
                    text: headset.tr("Copier le diagnostic")
                    icon.name: "edit-copy"
                    tooltip: headset.tr("Texte expurgé (ni adresse, ni numéro de série, ni noms) à joindre à un rapport")
                    onTriggered: {
                        clipboard.text = headset.diagnostic()
                        clipboard.selectAll()
                        clipboard.copy()
                        copied.visible = true
                    }
                },
                Kirigami.Action {
                    text: headset.tr("Assistant de premier lancement")
                    icon.name: "help-hint"
                    onTriggered: headset.showAssistant()
                },
                Kirigami.Action {
                    text: headset.tr("Guide d'utilisation")
                    icon.name: "help-contents"
                    onTriggered: Qt.openUrlExternally("https://asfax03l.github.io/openheadset-control/")
                },
                Kirigami.Action {
                    text: headset.tr("Signaler un problème…")
                    icon.name: "tools-report-bug"
                    onTriggered: {
                        reportDialog.diagnostic = headset.diagnostic()
                        reportDialog.open()
                    }
                }
            ]
            // Rapport de bogue : le diagnostic exact est relu avant d'ouvrir
            // le formulaire GitHub prérempli (rien n'est envoyé ici).
            Kirigami.PromptDialog {
                id: reportDialog
                property string diagnostic: ""
                title: headset.tr("Signaler un problème")
                subtitle: headset.tr("Ce texte sera prérempli dans le formulaire « Bogue » de GitHub, ouvert dans le navigateur. Relisez-le : rien n'est envoyé tant que vous ne validez pas le formulaire sur GitHub (compte GitHub nécessaire).")
                standardButtons: Kirigami.Dialog.NoButton
                QQC2.TextArea {
                    Accessible.name: headset.tr("Diagnostic à envoyer")
                    text: reportDialog.diagnostic
                    readOnly: true
                    wrapMode: TextEdit.Wrap
                    font: Kirigami.Theme.smallFont
                }
                customFooterActions: [
                    Kirigami.Action {
                        text: headset.tr("Ouvrir le formulaire sur GitHub")
                        icon.name: "internet-web-browser"
                        onTriggered: {
                            Qt.openUrlExternally(headset.issueUrl(reportDialog.diagnostic))
                            reportDialog.close()
                        }
                    },
                    Kirigami.Action {
                        text: headset.tr("Annuler")
                        icon.name: "dialog-cancel"
                        onTriggered: reportDialog.close()
                    }
                ]
            }
            // Presse-papiers : QML n'en a pas d'accès direct.
            TextEdit {
                id: clipboard
                visible: false
            }
            ColumnLayout {
            Kirigami.InlineMessage {
                id: copied
                Layout.fillWidth: true
                visible: false
                type: Kirigami.MessageType.Positive
                text: headset.tr("Diagnostic copié dans le presse-papiers.")
                showCloseButton: true
            }
            Kirigami.FormLayout {
                QQC2.Label {
                    Kirigami.FormData.label: headset.tr("Application :")
                    text: headset.tr("OpenHeadset Control (client Kirigami)")
                }
                QQC2.Label {
                    Kirigami.FormData.label: headset.tr("Casque :")
                    text: headset.title
                }
                QQC2.Label {
                    Kirigami.FormData.label: headset.tr("Micrologiciel :")
                    text: headset.firmware !== "" ? headset.firmware : headset.tr("non lu")
                }
                QQC2.Label {
                    Kirigami.FormData.label: headset.tr("Statut :")
                    text: headset.tr("Projet non officiel, sans lien avec Sennheiser ni Sonova")
                    wrapMode: Text.Wrap
                }
                QQC2.Label {
                    Kirigami.FormData.label: headset.tr("Licence :")
                    text: headset.tr("MIT OU Apache-2.0")
                }
            }
            }
        }
    }
}

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
    title: "OpenHeadset Control"
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
        title: "OpenHeadset Control"
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
                text: headset.busy ? "Demande en cours…" : headset.connected ? headset.batteryText : headset.connection
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
                QQC2.Label { text: "ANC ↔ transparence" }
                QQC2.Slider {
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
                QQC2.Label { text: "Anti-vent"; Layout.fillWidth: true }
                QQC2.Switch {
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
                QQC2.Label { text: "Profil personnalisé"; Layout.fillWidth: true }
                QQC2.Switch {
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
                text: "Ouvrir OpenHeadset Control"
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
                if (headset.antiWind === root.essaiAntiVent || interrupteur.checked === root.essaiAntiVent)
                    return root.essaiEchec("anti-vent simulé non relu à l'affichage")
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
        title: "OpenHeadset Control"
        // Logo installé par scripts/installer-linux.sh (thème d'icônes).
        titleIcon: "org.openheadset.control"
        // Bouton « Close Sidebar » non traduit : le tiroir se replie déjà
        // tout seul en fenêtre étroite.
        collapseButtonVisible: false
        actions: [
            Kirigami.Action {
                text: "Accueil"
                icon.name: "go-home"
                onTriggered: root.pageStack.replace(homePage)
            },
            Kirigami.Action {
                text: "Égaliseur"
                icon.name: "view-media-equalizer"
                onTriggered: root.pageStack.replace(soundPage)
            },
            Kirigami.Action {
                text: "Scènes"
                icon.name: "view-list-details"
                onTriggered: root.pageStack.replace(scenesPage)
            },
            Kirigami.Action {
                text: "Batterie"
                icon.name: "battery-good"
                onTriggered: root.pageStack.replace(batteryPage)
            },
            Kirigami.Action {
                text: "Paramètres"
                icon.name: "configure"
                onTriggered: root.pageStack.replace(settingsPage)
            },
            Kirigami.Action {
                text: "Connexion et audio"
                icon.name: "audio-card"
                onTriggered: root.pageStack.replace(audioPage)
            },
            Kirigami.Action {
                text: "À propos"
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
                text: headset.busy ? "Demande en cours…" : headset.connection
                elide: Text.ElideRight
            }
        }
    }

    // Accueil : casque, contrôle du bruit, fonctions rapides.
    Component {
        id: homePage
        Kirigami.ScrollablePage {
            objectName: "page-accueil"
            title: "Accueil"
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
                                text: "Batterie : " + headset.battery + " %"
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
                                text: "Micrologiciel " + headset.firmware + (headset.model !== "" ? " — " + headset.model : "")
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
                    text: "En attente du casque"
                    explanation: headset.waiting
                    helpfulAction: Kirigami.Action {
                        text: "Reconnecter le casque"
                        icon.name: "network-bluetooth"
                        enabled: headset.canReconnect
                        onTriggered: headset.reconnectHeadset()
                    }
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: Kirigami.Heading {
                        text: "Réduction de bruit"
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
                                Kirigami.FormData.label: "ANC ↔ transparence :"
                                QQC2.Slider {
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
                                id: antiWind
                                objectName: "accueil-anti-vent"
                                Kirigami.FormData.label: "Anti-vent :"
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
                        text: "Fonctionnalités rapides"
                        level: 3
                    }
                    contentItem: Kirigami.FormLayout {
                        QQC2.Switch {
                            id: multipoint
                            Kirigami.FormData.label: "Multipoint :"
                            text: "Deux appareils à la fois"
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
                            Kirigami.FormData.label: "Audio spatial :"
                            text: "Dolby Atmos"
                            checked: headset.spatial
                            enabled: headset.spatialEnabled
                            onToggled: {
                                headset.sendSpatial(checked)
                                root.confirmed(spatial, () => headset.spatial)
                            }
                        }
                        QQC2.Switch {
                            id: headTracking
                            Kirigami.FormData.label: "Suivi de tête :"
                            text: "Avec l'audio spatial"
                            checked: headset.headTracking
                            enabled: headset.headTrackingEnabled
                            onToggled: {
                                headset.sendHeadTracking(checked)
                                root.confirmed(headTracking, () => headset.headTracking)
                            }
                        }
                        QQC2.Switch {
                            id: touch
                            Kirigami.FormData.label: "Commandes tactiles :"
                            checked: headset.touch
                            enabled: headset.touchEnabled
                            onToggled: {
                                headset.sendTouch(checked)
                                root.confirmed(touch, () => headset.touch)
                            }
                        }
                        QQC2.Switch {
                            id: smartPause
                            Kirigami.FormData.label: "Smart Pause :"
                            text: "Pause au retrait du casque"
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
                        text: "Appareils appairés au casque"
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
                                        if (text === "Déconnecter") {
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
                title: "Déconnecter « " + deviceName + " » ?"
                subtitle: "Le casque coupe sa liaison avec cet appareil. Il reste appairé : « Connecter » le rétablit."
                standardButtons: Kirigami.Dialog.NoButton
                customFooterActions: [
                    Kirigami.Action {
                        text: "Déconnecter"
                        icon.name: "network-disconnect"
                        onTriggered: {
                            headset.toggleConnection(disconnectDialog.deviceIndex)
                            disconnectDialog.close()
                        }
                    },
                    Kirigami.Action {
                        text: "Annuler"
                        icon.name: "dialog-cancel"
                        onTriggered: disconnectDialog.close()
                    }
                ]
            }

            Kirigami.PromptDialog {
                id: multipointDialog
                title: "Couper le Multipoint ?"
                subtitle: "Sans Multipoint, le casque ne garde qu'un appareil : l'autre est déconnecté."
                standardButtons: Kirigami.Dialog.NoButton
                customFooterActions: [
                    Kirigami.Action {
                        text: "Couper le Multipoint"
                        icon.name: "dialog-ok"
                        onTriggered: {
                            headset.sendMultipoint(false)
                            multipointDialog.close()
                        }
                    },
                    Kirigami.Action {
                        text: "Annuler"
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
            title: "Égaliseur"
            ColumnLayout {
                spacing: Kirigami.Units.largeSpacing

                StatusHeader {}

                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    visible: !headset.connected
                    icon.name: "view-media-equalizer"
                    text: "Égaliseur indisponible"
                    explanation: headset.waiting
                }

                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    visible: headset.connected
                    header: Kirigami.Heading {
                        text: "Mode audio"
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
                            text: "Égaliseur graphique — " + headset.bandCount + " bandes"
                            level: 3
                        }
                        QQC2.Button {
                            text: "Réinitialiser"
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
                            text: "Amplification des basses"
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
                        text: "Préréglages"
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
                        text: "Préréglages et courbes"
                        level: 3
                    }
                    contentItem: ColumnLayout {
                        spacing: Kirigami.Units.smallSpacing
                        RowLayout {
                            QQC2.TextField {
                                id: presetName
                                Layout.fillWidth: true
                                placeholderText: "Nom du préréglage"
                                enabled: headset.writable
                            }
                            QQC2.Button {
                                text: "Enregistrer la courbe"
                                enabled: headset.writable && presetName.text.trim() !== ""
                                onClicked: {
                                    if (headset.savePreset(presetName.text))
                                        presetName.text = ""
                                }
                            }
                        }
                        QQC2.Button {
                            text: headset.userPreset === ""
                                ? "Supprimer le préréglage (choisir un préréglage personnel)"
                                : "Supprimer « " + headset.userPreset + " »"
                            enabled: headset.userPreset !== ""
                            onClicked: headset.deleteUserPreset()
                        }
                        RowLayout {
                            QQC2.TextField {
                                id: gainsText
                                Layout.fillWidth: true
                                placeholderText: "Gains en dB : -3,5 ; -1 ; 0 ; …"
                                enabled: headset.writable
                            }
                            QQC2.Button {
                                text: "Appliquer"
                                enabled: headset.writable && gainsText.text.trim() !== ""
                                onClicked: {
                                    if (headset.applyGainsText(gainsText.text))
                                        gainsText.text = ""
                                }
                            }
                        }
                        RowLayout {
                            QQC2.Button {
                                text: "Exporter la courbe"
                                onClicked: headset.exportCurve()
                            }
                            QQC2.Button {
                                text: "Importer une courbe…"
                                enabled: headset.writable
                                onClicked: curveDialog.open()
                            }
                        }
                        RowLayout {
                            QQC2.Button {
                                text: "Mémoriser A"
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
                            text: "Exporter le profil auditif"
                            enabled: headset.profileAvailable
                            onClicked: headset.exportProfile()
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            opacity: 0.7
                            text: headset.eqNotice !== "" ? headset.eqNotice
                                : "Préréglages enregistrés sur ce PC (prereglages.txt). Courbe exportée dans Documents ; une courbe d'un autre modèle ou d'un autre nombre de bandes est refusée. Le profil auditif (lecture seule, aucune restauration) est une donnée personnelle : ne pas le publier."
                        }
                        FileDialog {
                            id: curveDialog
                            title: "Importer une courbe d'égaliseur"
                            nameFilters: ["Courbes d'égaliseur (*.txt)"]
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
            title: "Paramètres"
            ColumnLayout {
                StatusHeader {}
                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        QQC2.Label {
                            text: "Sauvegarder et retrouver mes réglages"
                            font.bold: true
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            text: "Scènes, préréglages, raccourcis, panneau et apparence. Aucun profil auditif. Relancer l'interface après import ; un brouillon non enregistré n'est pas inclus."
                        }
                        QQC2.CheckBox {
                            id: settingsBattery
                            text: "Inclure aussi l'historique de batterie (export et import)"
                            onToggled: headset.cancelSettings()
                        }
                        QQC2.Button {
                            text: "Exporter les réglages…"
                            onClicked: { headset.cancelSettings(); settingsSave.open() }
                        }
                        QQC2.Button {
                            text: "Choisir un fichier à importer…"
                            onClicked: { headset.cancelSettings(); settingsOpen.open() }
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.WordWrap
                            text: headset.settingsNotice
                            textFormat: Text.PlainText
                        }
                        QQC2.Button {
                            text: "Confirmer cet import"
                            enabled: headset.settingsReady
                            onClicked: headset.confirmSettings()
                        }
                        QQC2.Button {
                            text: "Annuler l'import"
                            enabled: headset.settingsReady
                            onClicked: headset.cancelSettings()
                        }
                    }
                }
                FileDialog {
                    id: settingsSave
                    title: "Exporter vers un nouveau fichier"
                    fileMode: FileDialog.SaveFile
                    nameFilters: ["Réglages JSON (*.json)"]
                    defaultSuffix: "json"
                    onAccepted: headset.exportSettings(selectedFile.toString(), settingsBattery.checked)
                }
                FileDialog {
                    id: settingsOpen
                    title: "Choisir les réglages à examiner"
                    fileMode: FileDialog.OpenFile
                    nameFilters: ["Réglages JSON (*.json)"]
                    onAccepted: headset.previewSettings(selectedFile.toString(), settingsBattery.checked)
                }
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: "Application"
                    }
                    QQC2.Switch {
                        id: autostartSwitch
                        Kirigami.FormData.label: "Ouverture de session :"
                        text: "Lancer OpenHeadset Control (icône seule)"
                        checked: headset.autostart
                        onToggled: {
                            headset.setAutostartEnabled(checked)
                            root.confirmed(autostartSwitch, () => headset.autostart)
                        }
                    }
                    RowLayout {
                        Kirigami.FormData.label: "Thème :"
                        QQC2.RadioButton {
                            text: "Clair"
                            checked: headset.themeChoice === 0
                            onClicked: headset.chooseTheme(0)
                        }
                        QQC2.RadioButton {
                            text: "Sombre"
                            checked: headset.themeChoice === 1
                            onClicked: headset.chooseTheme(1)
                        }
                        QQC2.Label {
                            visible: headset.themeChoice < 0
                            text: "(pour l'instant celui de Plasma)"
                            opacity: 0.7
                        }
                    }
                    QQC2.Button {
                        Kirigami.FormData.label: "Raccourcis clavier :"
                        text: "Modifier dans KDE…"
                        icon.name: "preferences-desktop-keyboard-shortcut"
                        // Flatpak : Configuration du système > Raccourcis.
                        visible: !headset.sandboxed
                        onClicked: headset.openShortcutSettings()
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        opacity: 0.7
                        text: "Rubrique « OpenHeadset Control » : mode de bruit suivant (Ctrl+Alt+N), profil personnalisé (Ctrl+Alt+P), fenêtre (Ctrl+Alt+M), et une action par scène, sans raccourci au départ."
                    }
                }
                Kirigami.PlaceholderMessage {
                    Layout.fillWidth: true
                    visible: !headset.connected
                    icon.name: "configure"
                    text: "Réglages indisponibles"
                    explanation: headset.waiting
                }
                Kirigami.FormLayout {
                    Layout.fillWidth: true
                    visible: headset.connected

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: "Appels"
                    }
                    QQC2.Switch {
                        id: autoAnswer
                        Kirigami.FormData.label: "Réponse automatique :"
                        checked: headset.autoAnswer
                        enabled: headset.autoAnswerEnabled
                        onToggled: {
                            headset.sendAutoAnswer(checked)
                            root.confirmed(autoAnswer, () => headset.autoAnswer)
                        }
                    }
                    QQC2.Switch {
                        id: comfort
                        Kirigami.FormData.label: "Appels confortables :"
                        checked: headset.comfort
                        enabled: headset.comfortEnabled
                        onToggled: {
                            headset.sendComfort(checked)
                            root.confirmed(comfort, () => headset.comfort)
                        }
                    }

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: "Alimentation"
                    }
                    QQC2.ComboBox {
                        Kirigami.FormData.label: "Extinction automatique :"
                        model: headset.autoOffLabels
                        currentIndex: headset.autoOff
                        enabled: headset.autoOffEnabled
                        onActivated: index => {
                            headset.sendAutoOffIndex(index)
                            currentIndex = Qt.binding(() => headset.autoOff)
                        }
                    }
                    QQC2.Switch {
                        id: protection
                        Kirigami.FormData.label: "Protection de la batterie :"
                        checked: headset.batteryProtection
                        enabled: headset.batteryProtectionEnabled
                        onToggled: {
                            headset.sendBatteryProtection(checked)
                            root.confirmed(protection, () => headset.batteryProtection)
                        }
                    }

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: "Invites"
                    }
                    QQC2.ComboBox {
                        Kirigami.FormData.label: "Invites :"
                        model: headset.promptModes
                        currentIndex: headset.promptMode
                        enabled: headset.promptsEnabled
                        onActivated: index => {
                            headset.sendPromptModeIndex(index)
                            currentIndex = Qt.binding(() => headset.promptMode)
                        }
                    }
                    RowLayout {
                        Kirigami.FormData.label: "Volume des invites :"
                        QQC2.Slider {
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
                            text: headset.promptVolume < 0 ? "non lu" : Math.round(promptVolume.value) + " %"
                            Layout.minimumWidth: Kirigami.Units.gridUnit * 3
                        }
                    }

                    Kirigami.Separator {
                        Kirigami.FormData.isSection: true
                        Kirigami.FormData.label: "Nom du casque"
                    }
                    RowLayout {
                        Kirigami.FormData.label: "Nouveau nom :"
                        QQC2.TextField {
                            id: newName
                            Layout.fillWidth: true
                            placeholderText: headset.headsetName === "" ? "non lu" : headset.headsetName
                            enabled: headset.renameEnabled
                            maximumLength: 32
                            onTextEdited: nameProblem.text = ""
                        }
                        QQC2.Button {
                            text: "Renommer"
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
                        text: "Lettres sans accent, chiffres et espaces, 32 caractères au plus. Le système et le téléphone peuvent garder l'ancien nom jusqu'à la prochaine connexion."
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
            title: "Scènes"
            Component.onCompleted: headset.reloadScenes()
            actions: [
                Kirigami.Action {
                    text: "Modifier scenes.txt"
                    icon.name: "document-edit"
                    onTriggered: Qt.openUrlExternally(headset.scenesUrl)
                },
                Kirigami.Action {
                    text: "Relire"
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
                    text: "Chaque réglage d'une scène est appliqué puis relu sur le casque, un par un. Une étape refusée arrête la scène et l'affiche ici."
                }
                Kirigami.AbstractCard {
                    Layout.fillWidth: true
                    contentItem: ColumnLayout {
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            text: "Enregistrer les réglages déjà lus. Les valeurs non lues restent absentes ; aucun réglage du casque ne change."
                        }
                        QQC2.TextField {
                            id: captureSceneName
                            Layout.fillWidth: true
                            placeholderText: "Nom de la scène"
                            Accessible.name: "Nom de la scène à enregistrer"
                            maximumLength: 32
                        }
                        QQC2.CheckBox {
                            id: captureSceneReplace
                            text: "Remplacer la scène et son préréglage existants"
                        }
                        QQC2.Button {
                            text: "Enregistrer l'état actuel comme scène"
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
                                text: "Appliquer"
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
                        text: "Panneau rapide"
                        level: 3
                    }
                    contentItem: ColumnLayout {
                        QQC2.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            opacity: 0.7
                            text: "Ouvert par un clic sur l'icône de la zone de notification : batterie et anti-vent, plus ce qui est coché ici (quatre scènes au plus)."
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.largeSpacing
                            QQC2.CheckBox {
                                id: panelNoiseBox
                                text: "Contrôle du bruit"
                                checked: headset.panelNoise
                                onToggled: {
                                    headset.setPanelOption("bruit", checked)
                                    root.confirmed(panelNoiseBox, () => headset.panelNoise)
                                }
                            }
                            QQC2.CheckBox {
                                id: panelLevelBox
                                text: "Curseur ANC ↔ transparence"
                                checked: headset.panelLevel
                                onToggled: {
                                    headset.setPanelOption("niveau", checked)
                                    root.confirmed(panelLevelBox, () => headset.panelLevel)
                                }
                            }
                            QQC2.CheckBox {
                                id: panelProfileBox
                                text: "Profil personnalisé"
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
                            QQC2.Label { text: "Scènes :" }
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
                        text: "Modifier les scènes"
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
                            QQC2.Button { text: "Nouvelle"; icon.name: "list-add"; onClicked: headset.sceneEditAction("nouvelle") }
                            QQC2.Button { text: "Dupliquer"; icon.name: "edit-copy"; enabled: headset.sceneEditNames.length > 0; onClicked: headset.sceneEditAction("dupliquer") }
                            QQC2.Button { text: "Supprimer"; icon.name: "edit-delete"; enabled: headset.sceneEditNames.length > 0; onClicked: headset.sceneEditAction("supprimer") }
                            QQC2.Button { text: "Monter"; icon.name: "go-up"; enabled: headset.sceneEditSelected > 0; onClicked: headset.sceneEditAction("monter") }
                            QQC2.Button { text: "Descendre"; icon.name: "go-down"; enabled: headset.sceneEditSelected + 1 < headset.sceneEditNames.length; onClicked: headset.sceneEditAction("descendre") }
                        }
                        Kirigami.FormLayout {
                            Layout.fillWidth: true
                            visible: headset.sceneEditNames.length > 0
                            QQC2.TextField {
                                id: sceneName
                                Kirigami.FormData.label: "Nom :"
                                text: headset.sceneFormName
                                onTextEdited: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                id: sceneSound
                                Kirigami.FormData.label: "Mode audio :"
                                model: headset.sceneSoundChoices
                                currentIndex: headset.sceneFormSound
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                id: sceneSpatial
                                Kirigami.FormData.label: "Audio spatial :"
                                model: headset.sceneSwitchChoices
                                currentIndex: headset.sceneFormSpatial
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                id: sceneNoise
                                Kirigami.FormData.label: "Contrôle du bruit :"
                                model: headset.sceneNoiseChoices
                                currentIndex: headset.sceneFormNoise
                                onActivated: sceneEditor.send()
                            }
                            QQC2.TextField {
                                id: sceneLevel
                                Kirigami.FormData.label: "Niveau (0 à 100) :"
                                placeholderText: "vide = inchangé"
                                text: headset.sceneFormLevel
                                onTextEdited: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                id: sceneAntiWind
                                Kirigami.FormData.label: "Anti-vent :"
                                model: headset.sceneSwitchChoices
                                currentIndex: headset.sceneFormAntiWind
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                id: sceneTouch
                                Kirigami.FormData.label: "Commandes tactiles :"
                                model: headset.sceneSwitchChoices
                                currentIndex: headset.sceneFormTouch
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                id: sceneEq
                                Kirigami.FormData.label: "Préréglage d'égaliseur :"
                                model: headset.sceneEqChoices
                                currentIndex: headset.sceneFormEqualizer
                                onActivated: sceneEditor.send()
                            }
                            QQC2.ComboBox {
                                id: sceneBass
                                Kirigami.FormData.label: "Amplification des basses :"
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
                                text: "Enregistrer"
                                icon.name: "document-save"
                                enabled: headset.sceneEditCanSave
                                onClicked: headset.sceneEditAction("enregistrer")
                            }
                            QQC2.Button {
                                text: "Annuler les modifications"
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
                    opacity: 0.6
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
            title: "Connexion et audio"
            Component.onCompleted: headset.reloadAudio()
            actions: [Kirigami.Action {
                text: headset.audioReading ? "Lecture PipeWire…" : "Relire PipeWire"
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
                AudioSection { title: "Constats"; section: "constats" }
                AudioSection { title: "Pilotage du casque"; section: "pilotage" }
                AudioSection { title: "Sortie audio du système"; section: "systeme" }
                AudioSection { title: "Flux Bluetooth"; section: "flux" }
            }
        }
    }

    // Batterie : courbe enregistrée par le service, sur ce PC.
    Component {
        id: batteryPage
        Kirigami.ScrollablePage {
            objectName: "page-batterie"
            id: batteryScroll
            title: "Batterie"
            property string notice: ""
            Component.onCompleted: headset.reloadBattery(headset.batterySpan)
            actions: [
                Kirigami.Action {
                    text: "Exporter (CSV)"
                    icon.name: "document-export"
                    onTriggered: batteryScroll.notice = headset.exportBattery()
                },
                Kirigami.Action {
                    text: "Effacer l'historique…"
                    icon.name: "edit-clear-history"
                    onTriggered: clearBatteryDialog.open()
                }
            ]
            Kirigami.PromptDialog {
                id: clearBatteryDialog
                title: "Effacer l'historique de la batterie ?"
                subtitle: "Toutes les mesures enregistrées sur ce PC sont supprimées. Exporter d'abord pour les garder."
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
                        text: "Batterie : " + headset.batteryCurrent
                        level: 2
                    }
                    QQC2.ComboBox {
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
                        text: "Aucune mesure sur la période"
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
                        text: "maintenant"
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
            title: "À propos"
            actions: [
                Kirigami.Action {
                    text: "Copier le diagnostic"
                    icon.name: "edit-copy"
                    tooltip: "Texte expurgé (ni adresse, ni numéro de série, ni noms) à joindre à un rapport"
                    onTriggered: {
                        clipboard.text = headset.diagnostic()
                        clipboard.selectAll()
                        clipboard.copy()
                        copied.visible = true
                    }
                }
            ]
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
                text: "Diagnostic copié dans le presse-papiers."
                showCloseButton: true
            }
            Kirigami.FormLayout {
                QQC2.Label {
                    Kirigami.FormData.label: "Application :"
                    text: "OpenHeadset Control (client Kirigami)"
                }
                QQC2.Label {
                    Kirigami.FormData.label: "Casque :"
                    text: headset.title
                }
                QQC2.Label {
                    Kirigami.FormData.label: "Micrologiciel :"
                    text: headset.firmware !== "" ? headset.firmware : "non lu"
                }
                QQC2.Label {
                    Kirigami.FormData.label: "Statut :"
                    text: "Projet non officiel, sans lien avec Sennheiser ni Sonova"
                    wrapMode: Text.Wrap
                }
                QQC2.Label {
                    Kirigami.FormData.label: "Licence :"
                    text: "MIT OU Apache-2.0"
                }
            }
            }
        }
    }
}

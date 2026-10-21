// Widget Plasma d'OpenHeadset Control : client de l'API D-Bus du service
// momentum-daemon (org.openheadset.Control1). Il n'envoie que des commandes
// de l'API (liste fermée, validée par le service) et ne parle jamais au
// casque lui-même. Le service n'est pas démarré tant que l'utilisateur ne
// le demande pas : le widget seul n'ouvre aucune connexion Bluetooth.
import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import org.kde.plasma.components as PlasmaComponents
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.extras as PlasmaExtras
import org.kde.plasma.plasma5support as P5Support
import org.kde.plasma.plasmoid
import org.kde.plasma.workspace.dbus as DBus

PlasmoidItem {
    id: root

    readonly property string service: "org.openheadset.Control"
    readonly property string objectPath: "/org/openheadset/Control"
    readonly property string iface: "org.openheadset.Control1"

    // Snapshot du service (JSON de momentum_client::Snapshot), ou null.
    property var snapshot: null
    property string error: ""

    readonly property bool running: watcher.registered
    readonly property var headset: snapshot && snapshot.state ? snapshot.state : null
    readonly property string headsetName: snapshot
        ? ((snapshot.info && snapshot.info.name) || snapshot.device_name || "")
        : ""
    readonly property bool writable: headset !== null && !(snapshot && snapshot.read_only)
    readonly property int battery: headset ? headset.battery : -1
    // Même règle que momentum_core::protocol::messages::m5_noise_mode.
    readonly property string mode: !headset ? ""
        : !headset.noise.anc_enabled ? "Off"
        : headset.noise.adaptive ? "Adaptive" : "Custom"

    Plasmoid.icon: "org.openheadset.control"
    Plasmoid.status: running ? PlasmaCore.Types.ActiveStatus : PlasmaCore.Types.PassiveStatus
    toolTipMainText: headsetName || "OpenHeadset Control"
    toolTipSubText: {
        if (!running)
            return "Service arrêté";
        if (!headset)
            return snapshot && snapshot.device_name ? "Connexion au casque…" : "Recherche du casque…";
        const modes = { "Off": "réduction désactivée", "Adaptive": "ANC adaptatif", "Custom": "ANC personnalisé" };
        return "Batterie : " + battery + " %\n" + modes[mode];
    }

    // Le service apparaît ou disparaît (activation D-Bus, arrêt).
    DBus.DBusServiceWatcher {
        id: watcher
        busType: DBus.BusType.Session
        watchedService: root.service
        onRegisteredChanged: root.refresh()
    }

    // Événements du service : relecture immédiate de l'état.
    DBus.SignalWatcher {
        busType: DBus.BusType.Session
        service: root.service
        path: root.objectPath
        iface: root.iface
        enabled: root.running
        function dbusEvent(json) {
            root.refresh();
        }
    }

    // Relecture régulière, au cas où un événement serait manqué.
    Timer {
        interval: root.expanded ? 3000 : 30000
        running: root.running
        repeat: true
        triggeredOnStart: true
        onTriggered: root.refresh()
    }

    function call(member, args, onReply) {
        DBus.SessionBus.asyncCall({
            "service": root.service,
            "path": root.objectPath,
            "iface": root.iface,
            "member": member,
            "arguments": args,
            "signature": args.length ? "(s)" : ""
        }, function (reply) {
            root.error = "";
            if (onReply)
                onReply(reply);
        }, function (failure) {
            root.error = failure && failure.message ? failure.message : "service injoignable";
        });
    }

    function refresh() {
        if (!running) {
            snapshot = null;
            return;
        }
        call("Snapshot", [], function (reply) {
            try {
                root.snapshot = JSON.parse(reply.value);
            } catch (e) {
                root.error = "réponse illisible du service";
            }
        });
    }

    // Une commande de l'API, en JSON (voir momentum_client::Command) ; le
    // service la refuse si le casque est absent ou en lecture seule.
    function send(command) {
        call("Command", [new DBus.string(JSON.stringify(command))], function () {
            afterCommand.restart();
        });
    }

    // Le service accepte la commande puis le casque l'applique : l'état
    // relu arrive un peu après la réponse.
    Timer {
        id: afterCommand
        interval: 1500
        onTriggered: root.refresh()
    }

    // Démarre le service par activation D-Bus (premier appel).
    function startService() {
        call("Snapshot", [], function (reply) {
            root.snapshot = JSON.parse(reply.value);
        });
    }

    P5Support.DataSource {
        id: launcher
        engine: "executable"
        onNewData: (source, data) => disconnectSource(source)
    }

    // Fenêtre déjà ouverte : elle se montre ; sinon le client est lancé
    // (installation par script, sinon Flatpak).
    function openWindow() {
        DBus.SessionBus.asyncCall({
            "service": "org.openheadset.ControlWindow",
            "path": "/org/openheadset/ControlWindow",
            "iface": "org.openheadset.ControlWindow1",
            "member": "Show",
            "arguments": [],
            "signature": ""
        }, function () {}, function () {
            launcher.connectSource("setsid -f sh -c 'if [ -x \"$HOME/.local/bin/momentum-kirigami\" ]; then exec \"$HOME/.local/bin/momentum-kirigami\"; elif command -v momentum-kirigami >/dev/null; then exec momentum-kirigami; else exec flatpak run org.openheadset.Control; fi'");
        });
        root.expanded = false;
    }

    compactRepresentation: MouseArea {
        onClicked: root.expanded = !root.expanded
        Kirigami.Icon {
            anchors.fill: parent
            source: Plasmoid.icon
            fallback: "audio-headphones"
            opacity: root.headset ? 1 : 0.5
        }
        PlasmaComponents.Label {
            visible: root.battery >= 0 && parent.height >= Kirigami.Units.iconSizes.medium
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            text: root.battery + "%"
            font: Kirigami.Theme.smallFont
            color: root.battery > 20 ? Kirigami.Theme.textColor : Kirigami.Theme.negativeTextColor
        }
    }

    fullRepresentation: PlasmaExtras.Representation {
        Layout.minimumWidth: Kirigami.Units.gridUnit * 16
        Layout.minimumHeight: Kirigami.Units.gridUnit * 10
        Layout.preferredHeight: Kirigami.Units.gridUnit * 12

        header: PlasmaExtras.PlasmoidHeading {
            RowLayout {
                anchors.fill: parent
                Kirigami.Icon {
                    source: Plasmoid.icon
                    fallback: "audio-headphones"
                    Layout.preferredWidth: Kirigami.Units.iconSizes.medium
                    Layout.preferredHeight: Kirigami.Units.iconSizes.medium
                }
                PlasmaExtras.Heading {
                    level: 3
                    text: root.headsetName || "OpenHeadset Control"
                    elide: Text.ElideRight
                    Layout.fillWidth: true
                }
                PlasmaComponents.ToolButton {
                    icon.name: "configure"
                    text: "Ouvrir"
                    display: PlasmaComponents.AbstractButton.IconOnly
                    PlasmaComponents.ToolTip.text: "Ouvrir OpenHeadset Control"
                    PlasmaComponents.ToolTip.visible: hovered
                    onClicked: root.openWindow()
                }
            }
        }

        // Service arrêté, casque absent : un message et, au besoin, un bouton.
        PlasmaExtras.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 2
            visible: !root.headset
            iconName: "audio-headphones"
            text: !root.running ? "Service OpenHeadset Control arrêté"
                : root.snapshot && root.snapshot.device_name ? "Connexion au casque…" : "Recherche du casque…"
            explanation: root.error
            helpfulAction: !root.running ? startAction : null
            Kirigami.Action {
                id: startAction
                icon.name: "media-playback-start"
                text: "Démarrer le service"
                onTriggered: root.startService()
            }
        }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            visible: root.headset !== null
            spacing: Kirigami.Units.smallSpacing

            PlasmaComponents.Label {
                Layout.fillWidth: true
                visible: !!(root.snapshot && (root.snapshot.read_only || root.snapshot.problem))
                text: root.snapshot
                    ? (root.snapshot.read_only ? "Lecture seule : " + root.snapshot.read_only : (root.snapshot.problem || ""))
                    : ""
                color: Kirigami.Theme.negativeTextColor
                wrapMode: Text.WordWrap
            }

            RowLayout {
                Layout.fillWidth: true
                PlasmaComponents.Label {
                    text: "Batterie : " + root.battery + " %"
                }
                PlasmaComponents.ProgressBar {
                    Layout.fillWidth: true
                    from: 0
                    to: 100
                    value: Math.max(root.battery, 0)
                }
            }

            PlasmaExtras.Heading {
                level: 5
                text: "Réduction de bruit"
            }
            RowLayout {
                Layout.fillWidth: true
                Repeater {
                    model: [
                        { "mode": "Adaptive", "label": "Adaptatif" },
                        { "mode": "Custom", "label": "Personnalisé" },
                        { "mode": "Off", "label": "Arrêt" }
                    ]
                    PlasmaComponents.Button {
                        Layout.fillWidth: true
                        text: modelData.label
                        checkable: true
                        checked: root.mode === modelData.mode
                        enabled: root.writable
                        // L'état affiché reste celui relu sur le casque.
                        onClicked: {
                            checked = Qt.binding(() => root.mode === modelData.mode);
                            if (root.mode !== modelData.mode)
                                root.send({ "SetMode": modelData.mode });
                        }
                    }
                }
            }

            RowLayout {
                Layout.fillWidth: true
                visible: root.mode === "Custom"
                PlasmaComponents.Label {
                    text: "ANC"
                }
                PlasmaComponents.Slider {
                    id: level
                    Layout.fillWidth: true
                    from: 0
                    to: 100
                    stepSize: 1
                    enabled: root.writable
                    value: root.headset ? root.headset.noise.level : 0
                    // Envoi au relâchement seulement, comme les interfaces.
                    onPressedChanged: {
                        if (!pressed && root.headset && Math.round(value) !== root.headset.noise.level)
                            root.send({ "SetLevel": Math.round(value) });
                    }
                }
                PlasmaComponents.Label {
                    text: "Transparence"
                }
            }

            PlasmaComponents.Switch {
                text: "Anti-vent"
                enabled: root.writable
                checked: root.headset ? root.headset.noise.anti_wind : false
                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => root.headset ? root.headset.noise.anti_wind : false);
                    root.send({ "SetAntiWind": wanted });
                }
            }

            PlasmaComponents.Switch {
                text: "Audio spatial"
                visible: root.headset && root.headset.spatial_audio !== null
                enabled: root.writable
                checked: root.headset ? !!root.headset.spatial_audio : false
                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => root.headset ? !!root.headset.spatial_audio : false);
                    root.send({ "SetSpatial": wanted });
                }
            }

            Item {
                Layout.fillHeight: true
            }
        }
    }
}

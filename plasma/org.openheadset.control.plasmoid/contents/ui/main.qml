// Widget Plasma d'OpenHeadset Control : client de l'API D-Bus du service
// momentum-daemon (org.openheadset.Control1). Il n'envoie que des commandes
// de l'API (liste fermée, validée par le service) et ne parle jamais au
// casque lui-même. Le service n'est pas démarré tant que l'utilisateur ne
// le demande pas : le widget seul n'ouvre aucune connexion Bluetooth.
//
// Chaque réglage passe par une demande suivie (`Submit`) : le widget
// affiche son résultat, relu sur le casque, quand le service le publie
// (`Done`). Les commandes affichées gardent l'état confirmé par le casque.
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

    // Langue : celle de Plasma (français si elle commence par « fr »,
    // anglais sinon). Le widget, en QML seul, a son propre dictionnaire.
    readonly property bool francais: Qt.locale().name.startsWith("fr")
    readonly property var anglais: ({
        "Service arrêté": "Service stopped",
        "Connexion au casque…": "Connecting to the headphones…",
        "Recherche du casque…": "Searching for the headphones…",
        "réduction désactivée": "noise cancelling off",
        "ANC adaptatif": "adaptive ANC",
        "ANC personnalisé": "custom ANC",
        "Batterie : ": "Battery: ",
        "service injoignable": "service unreachable",
        "réponse illisible du service": "unreadable reply from the service",
        "appliqué et relu": "applied and read back",
        "déjà dans cet état": "already in this state",
        "accepté par le casque": "accepted by the headphones",
        "refusé": "refused",
        "non appliqué": "not applied",
        "annulé sans envoi": "cancelled without sending",
        "résultat inconnu": "unknown result",
        "refusé : ": "refused: ",
        "Ouvrir": "Open",
        "Ouvrir OpenHeadset Control": "Open OpenHeadset Control",
        "Service OpenHeadset Control arrêté": "OpenHeadset Control service stopped",
        "Reconnecter le casque": "Reconnect the headphones",
        "Démarrer le service": "Start the service",
        "Lecture seule : ": "Read-only: ",
        "Réduction de bruit": "Noise cancelling",
        "Adaptatif": "Adaptive",
        "Personnalisé": "Custom",
        "Arrêt": "Off",
        "Transparence": "Transparency",
        "Anti-vent": "Anti-wind",
        "Audio spatial": "Spatial audio",
        "Suivi de tête": "Head tracking",
        "Profil personnalisé": "Custom profile",
        "Scènes": "Scenes",
        "Envoi au casque…": "Sending to the headphones…"
    })
    function t(texte) {
        return francais ? texte : (anglais[texte] || texte);
    }

    // Snapshot du service (JSON de momentum_client::Snapshot), ou null.
    property var snapshot: null
    property string error: ""
    // Scènes montrées (celles du panneau rapide, `panneau.txt`).
    property var scenes: []
    // Demandes envoyées dont le résultat n'est pas encore arrivé.
    property var pending: []
    // Résultats arrivés avant le numéro de leur demande : [id, résultat].
    property var early: []
    // Dernier résultat d'une demande de ce widget.
    property string result: ""
    property bool resultOk: true

    readonly property bool running: watcher.registered
    readonly property bool busy: pending.length > 0
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
    // Même règle que momentum_core::hotkeys::profile_toggle : le profil
    // personnalisé est actif, ou reçu par le casque et sélectionnable.
    readonly property bool profileOn: !!headset && headset.audio_mode === "Personalization"
    readonly property bool profileAvailable: profileOn
        || (!!headset && headset.sound_profile === "Received")

    Plasmoid.icon: "org.openheadset.control"
    Plasmoid.status: running ? PlasmaCore.Types.ActiveStatus : PlasmaCore.Types.PassiveStatus
    toolTipMainText: headsetName || "OpenHeadset Control"
    toolTipSubText: {
        if (!running)
            return root.t("Service arrêté");
        if (!headset)
            return snapshot && snapshot.device_name ? root.t("Connexion au casque…") : root.t("Recherche du casque…");
        const modes = { "Off": root.t("réduction désactivée"), "Adaptive": root.t("ANC adaptatif"), "Custom": root.t("ANC personnalisé") };
        return root.t("Batterie : ") + battery + " %\n" + modes[mode];
    }

    // Le service apparaît ou disparaît (activation D-Bus, arrêt).
    DBus.DBusServiceWatcher {
        id: watcher
        busType: DBus.BusType.Session
        watchedService: root.service
        onRegisteredChanged: {
            root.pending = [];
            root.early = [];
            root.refresh();
        }
    }

    // Événements du service : résultat d'une demande, puis relecture de
    // l'état.
    DBus.SignalWatcher {
        busType: DBus.BusType.Session
        service: root.service
        path: root.objectPath
        iface: root.iface
        enabled: root.running
        function dbusEvent(json) {
            let event = null;
            try {
                event = JSON.parse(json);
            } catch (e) {}
            if (event && event.Done)
                root.finished(event.Done.id, event.Done.outcome);
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

    function call(member, args, signature, onReply, onFailure) {
        DBus.SessionBus.asyncCall({
            "service": root.service,
            "path": root.objectPath,
            "iface": root.iface,
            "member": member,
            "arguments": args,
            "signature": signature
        }, function (reply) {
            root.error = "";
            if (onReply)
                onReply(reply);
        }, function (failure) {
            const message = failure && failure.message ? failure.message : root.t("service injoignable");
            if (onFailure)
                onFailure(message);
            else
                root.error = message;
        });
    }

    function refresh() {
        if (!running) {
            snapshot = null;
            scenes = [];
            return;
        }
        call("Snapshot", [], "", function (reply) {
            try {
                root.snapshot = JSON.parse(reply.value);
            } catch (e) {
                root.error = root.t("réponse illisible du service");
            }
        });
        // Service plus ancien sans `Scenes` : pas de boutons de scènes.
        call("Scenes", [], "", function (reply) {
            try {
                root.scenes = JSON.parse(reply.value).panel || [];
            } catch (e) {
                root.scenes = [];
            }
        }, function () {
            root.scenes = [];
        });
    }

    // Texte d'un résultat (même formulation que Outcome::describe).
    function describe(outcome) {
        if (typeof outcome === "string") {
            return { "Applied": root.t("appliqué et relu"), "AlreadySatisfied": root.t("déjà dans cet état"),
                     "Acknowledged": root.t("accepté par le casque") }[outcome] || outcome;
        }
        if (outcome.SystemRequested) return outcome.SystemRequested;
        const kind = Object.keys(outcome)[0];
        const labels = { "Refused": root.t("refusé"), "NotApplied": root.t("non appliqué"),
                         "Cancelled": root.t("annulé sans envoi"), "Unknown": root.t("résultat inconnu") };
        return (labels[kind] || kind) + " : " + outcome[kind];
    }

    // Résultat publié par le service : seuls ceux des demandes de ce widget
    // sont affichés.
    function finished(id, outcome) {
        const index = pending.indexOf(String(id));
        if (index < 0) {
            // Peut-être une demande de ce widget dont le numéro n'est pas
            // encore arrivé : gardé un moment.
            early = early.concat([[String(id), outcome]]).slice(-16);
            return;
        }
        const rest = pending.slice();
        rest.splice(index, 1);
        pending = rest;
        show(outcome);
    }

    function show(outcome) {
        resultOk = typeof outcome === "string" || !!outcome.SystemRequested;
        result = describe(outcome);
        resultTimer.restart();
    }

    // Une commande de l'API, en JSON (voir momentum_client::Command), pour
    // la connexion affichée : refusée tout de suite si le casque est absent,
    // en lecture seule ou si la connexion a changé.
    function send(command) {
        const session = snapshot && snapshot.session ? snapshot.session : 0;
        result = "";
        call("Submit", [new DBus.string(JSON.stringify(command)), new DBus.uint64(session)], "(st)",
            function (reply) {
                const id = String(reply.value);
                const known = root.early.find(entry => entry[0] === id);
                if (known) {
                    root.early = root.early.filter(entry => entry[0] !== id);
                    root.show(known[1]);
                } else {
                    root.pending = root.pending.concat([id]);
                }
            }, function (message) {
                root.resultOk = false;
                root.result = root.t("refusé : ") + message;
                resultTimer.restart();
            });
    }

    // Un résultat réussi s'efface après quelques secondes ; un échec reste
    // jusqu'à la demande suivante.
    Timer {
        id: resultTimer
        interval: 4000
        onTriggered: {
            if (root.resultOk)
                root.result = "";
        }
    }

    // Démarre le service par activation D-Bus (premier appel).
    function startService() {
        call("Snapshot", [], "", function (reply) {
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
        Layout.minimumWidth: Kirigami.Units.gridUnit * 18
        Layout.minimumHeight: Kirigami.Units.gridUnit * 12
        Layout.preferredHeight: content.implicitHeight + Kirigami.Units.gridUnit * 4

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
                PlasmaComponents.BusyIndicator {
                    visible: root.busy
                    running: root.busy
                    Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
                    Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
                }
                PlasmaComponents.ToolButton {
                    icon.name: "configure"
                    text: root.t("Ouvrir")
                    display: PlasmaComponents.AbstractButton.IconOnly
                    PlasmaComponents.ToolTip.text: root.t("Ouvrir OpenHeadset Control")
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
            text: !root.running ? root.t("Service OpenHeadset Control arrêté")
                : root.snapshot && root.snapshot.device_name ? root.t("Connexion au casque…") : root.t("Recherche du casque…")
            explanation: root.result || root.error || (root.snapshot && root.snapshot.problem) || ""
            helpfulAction: !root.running ? startAction : reconnectAction
            Kirigami.Action {
                id: reconnectAction
                text: root.t("Reconnecter le casque")
                icon.name: "network-bluetooth"
                enabled: root.running && !root.busy
                onTriggered: root.send("ReconnectHeadset")
            }
            Kirigami.Action {
                id: startAction
                icon.name: "media-playback-start"
                text: root.t("Démarrer le service")
                onTriggered: root.startService()
            }
        }

        ColumnLayout {
            id: content
            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            visible: root.headset !== null
            spacing: Kirigami.Units.smallSpacing

            PlasmaComponents.Label {
                Layout.fillWidth: true
                visible: !!(root.snapshot && (root.snapshot.read_only || root.snapshot.problem))
                text: root.snapshot
                    ? (root.snapshot.read_only ? root.t("Lecture seule : ") + root.snapshot.read_only : (root.snapshot.problem || ""))
                    : ""
                color: Kirigami.Theme.negativeTextColor
                wrapMode: Text.WordWrap
            }

            RowLayout {
                Layout.fillWidth: true
                PlasmaComponents.Label {
                    text: root.t("Batterie : ") + root.battery + " %"
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
                text: root.t("Réduction de bruit")
            }
            RowLayout {
                Layout.fillWidth: true
                Repeater {
                    model: [
                        { "mode": "Adaptive", "label": root.t("Adaptatif") },
                        { "mode": "Custom", "label": root.t("Personnalisé") },
                        { "mode": "Off", "label": root.t("Arrêt") }
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
                        if (!pressed)
                            value = Qt.binding(() => root.headset ? root.headset.noise.level : 0);
                    }
                }
                PlasmaComponents.Label {
                    text: root.t("Transparence")
                }
            }

            PlasmaComponents.Switch {
                text: root.t("Anti-vent")
                enabled: root.writable
                checked: root.headset ? root.headset.noise.anti_wind : false
                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => root.headset ? root.headset.noise.anti_wind : false);
                    root.send({ "SetAntiWind": wanted });
                }
            }

            PlasmaComponents.Switch {
                text: root.t("Audio spatial")
                visible: root.headset && root.headset.spatial_audio !== null
                enabled: root.writable
                checked: root.headset ? !!root.headset.spatial_audio : false
                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => root.headset ? !!root.headset.spatial_audio : false);
                    root.send({ "SetSpatial": wanted });
                }
            }

            // Le suivi de tête exige l'audio spatial (règle du casque).
            PlasmaComponents.Switch {
                text: root.t("Suivi de tête")
                visible: root.headset && root.headset.head_tracking !== null
                enabled: root.writable && root.headset && !!root.headset.spatial_audio
                checked: root.headset ? !!root.headset.head_tracking : false
                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => root.headset ? !!root.headset.head_tracking : false);
                    root.send({ "SetHeadTracking": wanted });
                }
            }

            PlasmaComponents.Switch {
                text: root.t("Profil personnalisé")
                visible: root.profileAvailable
                enabled: root.writable
                checked: root.profileOn
                onToggled: {
                    const wanted = checked;
                    checked = Qt.binding(() => root.profileOn);
                    if (wanted !== root.profileOn)
                        root.send({ "SetSoundProfile": wanted });
                }
            }

            PlasmaExtras.Heading {
                level: 5
                text: root.t("Scènes")
                visible: root.scenes.length > 0
            }
            Flow {
                Layout.fillWidth: true
                visible: root.scenes.length > 0
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: root.scenes
                    PlasmaComponents.Button {
                        text: modelData
                        icon.name: "media-playback-start"
                        enabled: root.writable && !root.busy
                        onClicked: root.send({ "ApplyScene": modelData })
                    }
                }
            }

            // Résultat de la dernière demande, relu sur le casque.
            PlasmaComponents.Label {
                Layout.fillWidth: true
                visible: root.busy || root.result !== ""
                text: root.busy ? "Envoi au casque…" : root.result
                color: root.busy || root.resultOk ? Kirigami.Theme.disabledTextColor : Kirigami.Theme.negativeTextColor
                wrapMode: Text.WordWrap
            }

            Item {
                Layout.fillHeight: true
            }
        }
    }
}

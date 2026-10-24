import QtQuick

Item {
    id: root

    property string deploymentSet: "basic"
    property bool connected: false

    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight

    Text {
        id: label

        text: root.connected ? qsTr("Groundlink (%1)").arg(root.deploymentSet) : qsTr("Offline")
    }
}

# stock communication

```none
-> is for messages that are sent from the webapp to bridgething
<- is for messages that are sent from bridgething to the webapp

-------------------------------------------------------------------------------------
Make carthing discoverable/non-discoverable
-> {"type": "bluetooth", "action": "discoverable", "active": "false or true"}
<- no response needed, we can probs make our own we want

When a device starts trying to pair to the carthing, there's a handful of messages from bridgething to the webapp in this order:
<- {"address":"AA:AA:AA:AA:AA:AA","name":"Test","pin":"123456","type":"bluetooth_pin"} / Webapp displays: "Confirm that you see the code below on your phone". Pauses here till user confirms on phone
<- {"success":true,"type":"bluetooth_pairing_finished"} / Shown when user confirms on their phone
<- {"address":"AA:AA:AA:AA:AA:AA","connected":true,"type":"bluetooth_connection_status"} / Can be sent anytime to inform webapp of connection status
<- {"payload":[{"address":"AA:AA:AA:AA:AA:AA","blocked":false,"default":false,"device_info":{"name":"Test","type":"Android"}}],"type":"bluetooth_device_list"}
<- {"address":"AA:AA:AA:AA:AA:AA","name":"L's S22 Ultra","type":"bluetooth_current_device"} / Informs webapp what the current device is, whether or not it's connected
<- {"mac":"AA:AA:AA:AA:AA:AA","payload":true,"type":"transport_connection_status"} / Status of the connection between bridgething and the companion app
<- {"mac":"AA:AA:AA:AA:AA:AA","payload":true,"phone_type":"Android","type":"remote_control_connection_status"} / Same as transport status
-------------------------------------------------------------------------------------

-------------------------------------------------------------------------------------
Connect to specific device
-> {"type":"bluetooth","action":"select","mac":"AA:AA:AA:AA:AA:AA"}
<- responses are the same as the pairing responses excluding bluetooth_pin and bluetooth_pairing_finished
-------------------------------------------------------------------------------------

-------------------------------------------------------------------------------------
Get list of devices
-> {"type": "bluetooth", "action": "list"}
<- {"payload": [{"address": "AA:AA:AA:AA:AA:AA", "blocked": false, "default": true, "device_info": {"name": "Test1"}}, {"address": "BB:BB:BB:BB:BB:BB", "blocked": false, "default": false, "device_info": {"name": "Test2"}], "type": "bluetooth_device_list"}
-------------------------------------------------------------------------------------

-------------------------------------------------------------------------------------
Remove a device - Same response as get devices but without the device that was removed
-> {'type': 'bluetooth', 'action': 'forget', 'mac': 'AA:AA:AA:AA:AA:AA'}
<- {"payload":[{"address": "BB:BB:BB:BB:BB:BB", "blocked": false, "default": false, "device_info": {"name": "Test2"}],"type":"bluetooth_device_list"}
-------------------------------------------------------------------------------------
```

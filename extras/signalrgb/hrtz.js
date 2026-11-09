// hrtz for SignalRGB: SignalRGB sets the DPI light of the mice hrtz runs, with the rest of the
// PC's lighting. Needs hrtz running, with SignalRGB turned on under Sensor → Light.
// https://github.com/azaek/hrtz
//
// hrtz copies this file into SignalRGB's plugins folder. Each mouse hrtz lists on
// 127.0.0.1:17421 shows here as a one-LED device, and every frame the colour SignalRGB paints on
// it goes to hrtz over UDP. hrtz decides what reaches the mouse: a colour that holds, a pulse, or
// the light off. Nothing leaves this PC.
//
// Packet, 12 bytes: "HZ"; kind (1 hello, 2 frame, 3 goodbye); the mouse's id; the frame number
// (u32, big-endian); r, g, b; on a goodbye, 1 when SignalRGB is suspending.
//
// Written from SignalRGB's add-on API as Delido/signalrgb-wallpaper (MIT) uses it.

import udp from "@SignalRGB/udp";

const HOST = "127.0.0.1";
const PORT = 17421;
const MICE_URL = "http://" + HOST + ":" + PORT + "/mice";
const ID_PREFIX = "hrtz-";

const ICON = "data:image/svg+xml;utf8,"
    + "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'>"
    + "<rect x='18' y='6' width='28' height='52' rx='14' fill='%231b1f2a' stroke='%234a5160' stroke-width='2'/>"
    + "<line x1='32' y1='6' x2='32' y2='26' stroke='%234a5160' stroke-width='2'/>"
    + "<circle cx='32' cy='34' r='3' fill='%2342b4ff'/>"
    + "</svg>";

export function Name()                { return "hrtz"; }
export function Version()             { return "1.0.0"; }
export function Type()                { return "network"; }
export function Publisher()           { return "hrtz"; }
export function Size()                { return [1, 1]; }
export function DefaultPosition()     { return [50, 50]; }
export function DefaultScale()        { return 8.0; }
export function SubdeviceController() { return false; }
export function ImageUrl()            { return ICON; }
export function ControllableParameters() { return []; }

/* global
controller:readonly
*/

// Initialize, Render and Shutdown run once per device in one shared runtime, so each device keeps
// its own state, by its mouse's id.
const devices = new Map();

function current() {
    const id = (typeof controller !== "undefined" && controller && typeof controller.mouseId === "number")
        ? controller.mouseId
        : 0;
    let s = devices.get(id);
    if (!s) {
        s = { id: id, sock: null, frame: 0 };
        devices.set(id, s);
    }
    return s;
}

function send(s, kind, rgb, flag) {
    if (!s.sock) return;
    const n = s.frame;
    s.sock.send([
        0x48, 0x5a, kind, s.id & 0xff,
        (n >>> 24) & 0xff, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff,
        rgb[0] & 0xff, rgb[1] & 0xff, rgb[2] & 0xff,
        flag & 0xff,
    ]);
}

export function Initialize() {
    const s = current();
    device.setName(controller.name);
    device.setSize([1, 1]);
    device.setControllableLeds(["Light"], [[0, 0]]);
    try { device.setImageFromUrl(ICON); } catch (_) {}
    s.sock = udp.createSocket();
    try { s.sock.on("error", function (e) { device.log("hrtz: " + e); }); } catch (_) {}
    try { s.sock.on("connection", function () {}); } catch (_) {}
    try { s.sock.on("message", function () {}); } catch (_) {}
    s.sock.connect(HOST, PORT);
    send(s, 1, [0, 0, 0], 0);
}

export function Render() {
    const s = current();
    s.frame = (s.frame + 1) >>> 0;
    try {
        send(s, 2, device.color(0, 0), 0);
    } catch (e) {
        device.log("hrtz: " + e);
    }
}

export function Shutdown(suspend) {
    const s = current();
    try { send(s, 3, [0, 0, 0], suspend ? 1 : 0); } catch (_) {}
    try { s.sock.close(); } catch (_) {}
    s.sock = null;
}

class Mouse {
    constructor(id, name) {
        this.id = ID_PREFIX + id;
        this.mouseId = id;
        this.name = name;
        this.ip = HOST;
        this.image = ICON;
        this.announced = false;
    }

    // A controller becomes a device only once it's announced.
    update() {
        if (this.announced) return;
        this.announced = true;
        service.updateController(this);
        service.announceController(this);
    }
}

export function DiscoveryService() {
    this.IconUrl = ICON;
    this.askedAt = 0;
    this.misses = 0;

    // The add-on sandbox has synchronous XMLHttpRequest, so one tick gets its answer.
    const askMice = function () {
        try {
            const xhr = new XMLHttpRequest();
            xhr.open("GET", MICE_URL, false);
            xhr.timeout = 500;
            xhr.send(null);
            if (xhr.status === 200) {
                const list = JSON.parse(xhr.responseText);
                if (Array.isArray(list)) return list;
            }
        } catch (_) {
            // hrtz isn't running, or its SignalRGB switch is off.
        }
        return null;
    };

    this.sync = function () {
        let mice = askMice();
        if (mice === null) {
            // One missed answer can be a busy moment; two in a row (4 s) means hrtz is gone.
            this.misses++;
            if (this.misses < 2) return;
            mice = [];
        } else {
            this.misses = 0;
        }
        const listed = {};
        for (const m of mice) {
            if (typeof m.id !== "number") continue;
            listed[ID_PREFIX + m.id] = true;
            if (service.getController(ID_PREFIX + m.id) === undefined) {
                service.log("hrtz: adding " + m.name);
                service.addController(new Mouse(m.id, String(m.name)));
            }
        }
        for (const c of service.controllers.slice()) {
            if (c && c.obj && !listed[c.obj.id]) {
                try {
                    service.removeController(c);
                    service.log("hrtz: removed " + c.obj.name);
                } catch (e) {
                    service.log("hrtz: couldn't remove " + c.obj.name + ": " + e);
                }
            }
        }
    };

    this.Initialize = function () {
        this.sync();
    };

    this.Update = function () {
        for (const c of service.controllers) {
            if (c && c.obj && typeof c.obj.update === "function") c.obj.update();
        }
        const now = Date.now();
        if (now - this.askedAt < 2000) return;
        this.askedAt = now;
        this.sync();
    };

    this.Shutdown = function () {};
    this.Discovered = function () {};
    this.Removal = function () {};
}

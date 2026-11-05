// The display over USB from the browser (Web Serial): esptool-js writes the firmware, then the
// same line protocol as scripts/provision.py (INFO, PROVISION {json}, ERASE) sets it up.
import { ESPLoader, Transport } from 'esptool-js';

const ESPRESSIF = 0x303a;

export const supported = typeof navigator !== 'undefined' && 'serial' in navigator;

export interface Part { path: string; offset: number; size: number; sha256: string }
export interface Manifest { name: string; version: string; commit: string; built: string; chip: string; app: string; parts: Part[] }

/** What the display reports to INFO. Secrets only as whether they're set. */
export interface Info { project: string; version: string; mac: string; wifi: boolean; ha_url: string; ha_token: boolean; mqtt?: boolean; ip?: string }

export type Values = Partial<Record<'ssid' | 'password' | 'ha_url' | 'ha_token' | 'mqtt_url', string>>;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function loadManifest(): Promise<Manifest | null> {
  const r = await fetch('./firmware/manifest.json', { cache: 'no-cache' }).catch(() => null);
  return r?.ok ? r.json() : null;
}

export function pickPort(): Promise<SerialPort> {
  return navigator.serial.requestPort({ filters: [{ usbVendorId: ESPRESSIF }] });
}

/** The display's port without asking again, when this page already has permission for exactly one.
 *  After flashing it restarts and comes back as a new port, so this waits up to `waitMs` for it. */
export async function knownPort(waitMs = 0): Promise<SerialPort | null> {
  const end = Date.now() + waitMs;
  for (;;) {
    const ports = (await navigator.serial.getPorts()).filter(
      (p) => p.getInfo().usbVendorId === ESPRESSIF && (p as { connected?: boolean }).connected !== false
    );
    if (ports.length === 1) return ports[0];
    if (ports.length > 1 || Date.now() >= end) return null;
    await sleep(500);
  }
}

async function sha256(data: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', data as Uint8Array<ArrayBuffer>);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Writes every part of the manifest, then restarts the display into the new firmware.
 *  Without `erase`, the settings partition is left alone, so an update keeps Wi-Fi and the rest. */
export async function install(
  port: SerialPort,
  manifest: Manifest,
  opts: { erase: boolean; progress: (fraction: number) => void; log: (line: string) => void }
): Promise<void> {
  const fileArray = await Promise.all(
    manifest.parts.map(async (part) => {
      const r = await fetch(`./firmware/${part.path}`, { cache: 'no-cache' });
      if (!r.ok) throw new Error(`Couldn't download ${part.path} (${r.status}).`);
      const data = new Uint8Array(await r.arrayBuffer());
      if ((await sha256(data)) !== part.sha256) throw new Error(`${part.path} didn't download intact. Reload and try again.`);
      return { data, address: part.offset };
    })
  );
  const total = fileArray.reduce((n, f) => n + f.data.length, 0);
  const before = fileArray.map((_, i) => fileArray.slice(0, i).reduce((n, f) => n + f.data.length, 0));

  const transport = new Transport(port, false);
  const loader = new ESPLoader({
    transport,
    baudrate: 921600,
    terminal: { clean() {}, writeLine: (s) => opts.log(s), write: (s) => opts.log(s) }
  });
  try {
    const chip = await loader.main();
    if (!/ESP32-S3/i.test(chip)) throw new Error(`That board is an ${chip}; this firmware is for the ESP32-S3.`);
    await loader.writeFlash({
      fileArray,
      flashMode: 'keep',
      flashFreq: 'keep',
      flashSize: 'keep',
      eraseAll: opts.erase,
      compress: true,
      reportProgress: (i, written, size) => opts.progress((before[i] + fileArray[i].data.length * (written / size)) / total)
    });
    opts.progress(1);
    // esptool-js's hard_reset only drops RTS, which on the USB Serial/JTAG port leaves the chip
    // waiting in the bootloader. A full pulse with DTR off restarts it into the new firmware.
    await transport.setDTR(false);
    await transport.setRTS(true);
    await sleep(100);
    await transport.setRTS(false);
  } finally {
    await transport.disconnect().catch(() => {});
  }
}

/** A line-at-a-time conversation with the running firmware's USB console. */
export class Console {
  private buffer = '';
  private waiters: { test: (line: string) => boolean; done: (line: string) => void }[] = [];
  private reader: ReadableStreamDefaultReader<Uint8Array> | null = null;
  private reading: Promise<void> = Promise.resolve();

  private constructor(private port: SerialPort, private onLine: (line: string) => void) {}

  static async open(port: SerialPort, onLine: (line: string) => void): Promise<Console> {
    await port.open({ baudRate: 115200 });
    // DTR on, RTS off: the ESP32-S3 stays running (scripts/provision.py does the same).
    await port.setSignals({ dataTerminalReady: true, requestToSend: false });
    const c = new Console(port, onLine);
    c.reading = c.readLoop();
    return c;
  }

  private async readLoop() {
    const decoder = new TextDecoder();
    while (this.port.readable) {
      this.reader = this.port.readable.getReader();
      try {
        for (;;) {
          const { value, done } = await this.reader.read();
          if (done) return;
          this.buffer += decoder.decode(value, { stream: true });
          let nl: number;
          while ((nl = this.buffer.indexOf('\n')) >= 0) {
            const line = this.buffer.slice(0, nl).replace(/\r$/, '');
            this.buffer = this.buffer.slice(nl + 1);
            this.onLine(line);
            this.waiters = this.waiters.filter((w) => !(w.test(line) && (w.done(line), true)));
          }
        }
      } catch {
        return; // unplugged or closed
      } finally {
        this.reader.releaseLock();
        this.reader = null;
      }
    }
  }

  async send(line: string) {
    const writer = this.port.writable!.getWriter();
    try {
      await writer.write(new TextEncoder().encode(`\n${line}\n`));
    } finally {
      writer.releaseLock();
    }
  }

  /** The first line that passes `test` within `ms`, or null. */
  waitFor(test: (line: string) => boolean, ms: number): Promise<string | null> {
    return new Promise((resolve) => {
      const w = { test, done: (line: string) => { clearTimeout(timer); resolve(line); } };
      const timer = setTimeout(() => { this.waiters = this.waiters.filter((x) => x !== w); resolve(null); }, ms);
      this.waiters.push(w);
    });
  }

  /** Asks INFO until the firmware answers: right after flashing it's still starting up. */
  async info(ms = 20000): Promise<Info | null> {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      const answer = this.waitFor((l) => l.startsWith('INFO '), 1500);
      await this.send('INFO').catch(() => {});
      const line = await answer;
      if (line) return JSON.parse(line.slice(5));
    }
    return null;
  }

  /** 'ok', or the firmware's reason: wifi, url, token, format, storage; 'timeout' without an answer. */
  async provision(values: Values): Promise<string> {
    const answer = this.waitFor((l) => l.includes('PROVISION_OK') || l.includes('PROVISION_REJECTED'), 12000);
    await this.send(`PROVISION ${JSON.stringify(values)}`);
    const line = await answer;
    if (!line) return 'timeout';
    return line.includes('PROVISION_OK') ? 'ok' : line.split('PROVISION_REJECTED')[1].trim().slice(0, 32) || 'format';
  }

  async erase(): Promise<boolean> {
    const answer = this.waitFor((l) => l.includes('ERASE_OK'), 12000);
    await this.send('ERASE');
    return (await answer) !== null;
  }

  async close() {
    await this.reader?.cancel().catch(() => {});
    await this.reading;
    await this.port.close().catch(() => {});
  }
}

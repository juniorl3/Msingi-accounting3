// Receipt header details and printer preferences.
// Business details live in the settings table (per business). Paper width and auto-print are per computer (the printer is), so they use localStorage.
import { db } from "./db";
import { getActiveBusiness } from "./business";
import type { Receipt, PaperWidth, ReceiptBusiness } from "./receipt";
import type { DrawerPin, DrawerRule } from "./escpos";

export type ReceiptDetails = Omit<ReceiptBusiness, "name">;
const FIELDS = ["address", "phone", "tin", "footer"] as const;
const MAX: Record<(typeof FIELDS)[number], number> = { address: 120, phone: 40, tin: 30, footer: 160 };

export async function getReceiptBusiness(): Promise<ReceiptBusiness> {
  const biz = await getActiveBusiness(), d = await db();
  const rows = await d.select<Array<{ key: string; value: string }>>("SELECT key,value FROM settings WHERE key LIKE ?", [`receipt_${biz.id}_%`]);
  const get = (f: string) => rows.find(r => r.key === `receipt_${biz.id}_${f}`)?.value ?? "";
  return { name: biz.name, address: get("address"), phone: get("phone"), tin: get("tin"), footer: get("footer") || "Thank you for your business" };
}
export async function saveReceiptDetails(v: ReceiptDetails) {
  const biz = await getActiveBusiness(), d = await db();
  for (const f of FIELDS) {
    const val = v[f].trim().replace(/[\r\t]+/g, " ");
    if (val.length > MAX[f]) throw new Error(`The ${f} is too long (maximum ${MAX[f]} characters).`);
    await d.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [`receipt_${biz.id}_${f}`, val]);
  }
}

/** system = the normal print dialog; network = ESC/POS straight to the printer's IP address; queue = ESC/POS to a print queue on this computer (USB). */
export type PrintMode = "system" | "network" | "queue";
export type PrinterPrefs = { paper: PaperWidth; autoPrint: boolean; mode: PrintMode; host: string; port: number; queue: string; cut: boolean; drawerRule: DrawerRule; drawerPin: DrawerPin };
export const DEFAULT_PREFS: PrinterPrefs = { paper: "80", autoPrint: false, mode: "system", host: "", port: 9100, queue: "", cut: true, drawerRule: "off", drawerPin: 2 };
/** Accepts whatever is stored and returns safe values, so a damaged or older settings entry never breaks printing. */
export function cleanPrefs(p: Partial<Record<keyof PrinterPrefs, unknown>>): PrinterPrefs {
  const port = Number(p.port);
  return {
    paper: p.paper === "58" ? "58" : "80", autoPrint: p.autoPrint === true,
    mode: p.mode === "network" || p.mode === "queue" ? p.mode : "system",
    host: typeof p.host === "string" ? p.host.trim().slice(0, 253) : "", port: Number.isInteger(port) && port >= 1 && port <= 65535 ? port : 9100,
    queue: typeof p.queue === "string" ? p.queue.trim().slice(0, 127) : "", cut: p.cut !== false,
    drawerRule: p.drawerRule === "cash" || p.drawerRule === "always" ? p.drawerRule : "off", drawerPin: p.drawerPin === 5 ? 5 : 2,
  };
}
const PREF_KEY = "msingi_receipt_prefs", RECENT_KEY = "msingi_recent_receipts";
export function loadPrefs(): PrinterPrefs {
  try { return cleanPrefs(JSON.parse(localStorage.getItem(PREF_KEY) ?? "{}")); } catch { return { ...DEFAULT_PREFS }; }
}
export function savePrefs(p: PrinterPrefs) { try { localStorage.setItem(PREF_KEY, JSON.stringify(p)); } catch { /* storage unavailable: preferences just reset next time */ } }
export function loadRecent(): Receipt[] { try { const a = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]"); return Array.isArray(a) ? a : []; } catch { return []; } }
/** Replaces a saved recent receipt (same number) with an updated copy, e.g. after a fiscal receipt number was added. */
export function updateRecent(r: Receipt): Receipt[] {
  const next = loadRecent().map(x => (x.number === r.number && x.business.name === r.business.name ? r : x));
  try { localStorage.setItem(RECENT_KEY, JSON.stringify(next)); } catch { /* ignore */ }
  return next;
}
export function pushRecent(r: Receipt): Receipt[] {
  const next = [r, ...loadRecent().filter(x => !(x.number === r.number && x.business.name === r.business.name))].slice(0, 20);
  try { localStorage.setItem(RECENT_KEY, JSON.stringify(next)); } catch { /* ignore */ }
  return next;
}

// Bridge from the till screens to the printer commands in the Tauri backend.
import { invoke } from "@tauri-apps/api/core";
import { getActiveBusiness } from "./business";
import { db } from "./db";
import { drawerOnly, testPage } from "./escpos";
import { recordAction } from "./posLog";
import type { Actor } from "./posLog";
import type { PrinterPrefs } from "./receiptDb";

export const usesRawPrinting = (p: PrinterPrefs) => p.mode !== "system";

/** Sends ESC/POS bytes to the configured printer. Throws a readable message if it cannot. */
export async function sendBytes(p: PrinterPrefs, bytes: Uint8Array): Promise<void> {
  if (p.mode === "system") throw new Error("Choose a network or USB receipt printer in the printer settings first.");
  const target = p.mode === "network" ? p.host : p.queue;
  if (!target) throw new Error(p.mode === "network" ? "Enter the printer's IP address in the printer settings." : "Choose the printer's print queue in the printer settings.");
  try { await invoke("print_raw", { mode: p.mode, target, port: p.port, data: Array.from(bytes) }); }
  catch (e) { throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : "Printing failed."); }
}
export const listPrintQueues = () => invoke<string[]>("list_print_queues");

export const printTestPage = (p: PrinterPrefs) => sendBytes(p, testPage(p.paper, { cut: p.cut }));
export function kickDrawer(p: PrinterPrefs) {
  if (p.mode === "system") throw new Error("The cash drawer opens through a network or USB receipt printer. Choose one in the printer settings.");
  return sendBytes(p, drawerOnly(p.drawerPin));
}

/** Opens the drawer with no sale (change, float top-up). The signed-in user, and any approving manager, are recorded. */
export async function openDrawerNoSale(p: PrinterPrefs, actor: Actor): Promise<void> {
  await kickDrawer(p);
  const biz = await getActiveBusiness();
  await recordAction(await db(), { businessId: biz.id, kind: "drawer", userId: actor.userId, approverId: actor.approverId, shiftId: actor.shiftId, reason: "No sale" });
}

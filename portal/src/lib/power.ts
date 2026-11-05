// Sizing a 5 V supply for the panel's current limit. The limit covers the
// panel (crates/app/src/art.rs estimates it); the board adds its own draw on top.

export const BOARD_MA = 300; // ESP32-S3 with Wi-Fi and the mics, at its peaks
export const HEADROOM = 1.1; // run a supply below its rating
export const PORT_MAX_A = 3; // the board's USB-C POWER port
const RATINGS_A = [2, 2.5, 3, 4, 5, 6];

/** The smallest common 5 V rating that covers a current limit, in amps. */
export function suggestedSupplyA(limitMa: number): number {
  const needA = ((limitMa + BOARD_MA) * HEADROOM) / 1000;
  return RATINGS_A.find((a) => a >= needA - 1e-9) ?? Math.ceil(needA);
}

/** The highest current limit (in 100 mA steps) a supply of this rating covers. */
export function maxLimitMa(supplyA: number): number {
  return Math.floor((supplyA * 1000) / HEADROOM / 100 - BOARD_MA / 100) * 100;
}

// The citizen card: a 1200×675 image (the size X shows in full) of a holder's standing in a
// city, drawn in the browser so nothing is sent anywhere.
import romArt from "./art/rom.svg?raw";
import remArt from "./art/rem.svg?raw";

export type CardInput = {
  side: "rom" | "rem";
  rank: string;
  days: number;
  burnPct: number;
  next: string | null;
  nextDays: number | null;
  owner: string;
  cityBurned: string;
  site: string;
};

const CITY = {
  rom: { name: "Romulus", ticker: "ROM", hill: "THE PALATINE", deep: "#4a0f12", main: "#7a1e22", art: romArt },
  rem: { name: "Remus", ticker: "REM", hill: "THE AVENTINE", deep: "#10223a", main: "#2c4a70", art: remArt },
};

function image(svg: string): Promise<HTMLImageElement> {
  return new Promise((ok, fail) => {
    const img = new Image();
    img.onload = () => ok(img);
    img.onerror = fail;
    // An SVG drawn as an image needs its namespace, which inline SVG in the page does without.
    const full = svg.includes("xmlns=") ? svg : svg.replace("<svg", '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"');
    img.src = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(full)}`;
  });
}

export async function drawCard(c: CardInput): Promise<Blob> {
  const city = CITY[c.side];
  const W = 1200, H = 675;
  const canvas = document.createElement("canvas");
  canvas.width = W;
  canvas.height = H;
  const g = canvas.getContext("2d")!;
  try {
    await document.fonts.ready;
  } catch {
    /* fall back to the system serif */
  }
  const display = '"Cinzel", Georgia, serif';
  const body = '"Alegreya Sans", "Segoe UI", sans-serif';

  // The night over the Tiber, the city's colour rising on the left.
  const bg = g.createLinearGradient(0, 0, W, H);
  bg.addColorStop(0, city.main);
  bg.addColorStop(0.45, city.deep);
  bg.addColorStop(1, "#171b31");
  g.fillStyle = bg;
  g.fillRect(0, 0, W, H);
  g.strokeStyle = "#b07a3a";
  g.lineWidth = 6;
  g.strokeRect(24, 24, W - 48, H - 48);
  g.lineWidth = 1.5;
  g.strokeRect(38, 38, W - 76, H - 76);

  // The city's coin.
  try {
    const coin = await image(city.art);
    g.save();
    g.shadowColor = "rgba(0,0,0,0.5)";
    g.shadowBlur = 30;
    g.drawImage(coin, 80, 137, 400, 400);
    g.restore();
  } catch {
    /* the card still reads without it */
  }

  const x = 540;
  g.fillStyle = "#e9a04b";
  g.font = `600 26px ${display}`;
  g.fillText(`${city.hill} · ${city.ticker}`, x, 150);

  g.fillStyle = "#f3d9a4";
  g.font = `700 64px ${display}`;
  g.fillText(c.rank, x, 232);
  g.fillStyle = "#e7e1d6";
  g.font = `500 44px ${display}`;
  g.fillText(`of ${city.name}`, x, 290);

  g.font = `400 32px ${body}`;
  g.fillStyle = "#e7e1d6";
  const lines = [
    c.days === 0 ? "Joined the city today" : `Held ${c.days} ${c.days === 1 ? "day" : "days"} on average`,
    `Selling now burns ${c.burnPct}%`,
    c.next && c.nextDays !== null ? `${c.next} in ${c.nextDays} ${c.nextDays === 1 ? "day" : "days"}` : "The highest rank in the city",
    `${c.cityBurned} ${city.ticker} burned so far`,
  ];
  // Every line fits inside the border, shrinking a long one rather than cutting it.
  const room = W - 70 - x;
  lines.forEach((l, i) => {
    let size = 32;
    g.font = `400 ${size}px ${body}`;
    while (g.measureText(l).width > room && size > 20) g.font = `400 ${--size}px ${body}`;
    g.fillText(l, x, 362 + i * 50);
  });

  g.fillStyle = "#b9b2a6";
  g.font = `400 24px ${body}`;
  g.fillText(`${c.owner.slice(0, 4)}…${c.owner.slice(-4)}`, x, 590);
  g.textAlign = "right";
  g.fillText(c.site, W - 70, 590);

  return new Promise((ok, fail) => canvas.toBlob((b) => (b ? ok(b) : fail(new Error("no image"))), "image/png"));
}

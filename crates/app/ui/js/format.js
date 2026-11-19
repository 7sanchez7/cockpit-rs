// Nombres, argent, dates et durées tels que l'UI les écrit, au format français (`48 200`, `7,10 €`, `−4 %`).
// Port de Format.swift. Les dates sont lues en heure de Paris. Une fonction qui reçoit null rend null.
//
//   vide, moins
//   entier(v), decimal(v, d), compact(v), delta(v, compacte?), unites(ecart), nombre(v, mot)
//   pourcent(ratio, decimales?), variation(actuel, avant), evolution(actuel, avant), ecart(actuel, avant), points(actuel, avant)
//   signe(v, format), euros(v), dollars(v), dontVoix(v), rang(p)
//   heure(d), jourCourt(d), jourCourtCle("2026-11-17"), jourHeure(d), jourSemaine(d), dateLongue(d), moisAnnee(d)
//   ilYa(d, maintenant?), age(d, maintenant?), cadence(s), duree(s), cumul(s), passage(s)
// Une date est une Date, un horodatage en ms ou une chaîne ISO.

export const vide = "…";
export const moins = "−";
const fuseau = "Europe/Paris";
const langue = "fr-FR";

const nombres = new Map();
function nf(min, max) {
  const cle = min + ":" + max;
  if (!nombres.has(cle)) nombres.set(cle, new Intl.NumberFormat(langue, { minimumFractionDigits: min, maximumFractionDigits: max }));
  return nombres.get(cle);
}

// arrondi au plus proche, la moitié loin de zéro comme Swift
function arrondi(v) {
  const r = Math.sign(v) * Math.round(Math.abs(v));
  return r === 0 ? 0 : r;
}

export function entier(v) {
  return nf(0, 0).format(arrondi(v));
}

export function decimal(v, decimales) {
  return nf(decimales, decimales).format(v);
}

/** `6 420`, `25,4 k`, `48 k`, `337 k`, `1,2 M` : une décimale sous 100 k, pour qu'un axe serré ne répète pas `26 k`. */
export function compact(v) {
  const a = Math.abs(v);
  const s = v < 0 ? moins : "";
  if (a < 10_000) return s + entier(a);
  if (a < 100_000) return s + nf(0, 1).format(a / 1000) + " k";
  if (a < 1_000_000) return s + entier(a / 1000) + " k";
  return s + nf(0, 1).format(a / 1_000_000) + " M";
}

/** `+412`, `−96`, `+0`. */
export function delta(v, compacte = false) {
  const corps = compacte ? compact(Math.abs(v)) : entier(Math.abs(v));
  return (v < 0 ? moins : "+") + corps;
}

/** Un écart en unités, `= 0` s'il est nul. */
export function unites(ecartV) {
  if (ecartV == null) return null;
  return ecartV === 0 ? "= 0" : delta(ecartV);
}

/** `3 essais`, `1 achat`, `0 rond` : le pluriel commence à 2. */
export function nombre(v, mot) {
  if (v == null) return null;
  return entier(v) + " " + mot + (Math.abs(v) < 2 ? "" : "s");
}

/** `0,99 %` pour 0,0099. */
export function pourcent(ratio, decimales = 0) {
  return decimal(ratio * 100, decimales) + " %";
}

/** Variation relative : `+9 %`, `−4 %` ; null si la référence est nulle. */
export function variation(actuel, avant) {
  if (avant === 0) return null;
  const r = arrondi(((actuel - avant) / avant) * 100);
  return (r < 0 ? moins : "+") + entier(Math.abs(r)) + " %";
}

/** Variation entre deux relevés facultatifs : `= 0` si les deux sont nuls, null s'il en manque un. */
export function evolution(actuel, avant) {
  if (actuel == null || avant == null) return null;
  return variation(actuel, avant) ?? (actuel === 0 ? "= 0" : null);
}

/** Écart brut entre deux relevés facultatifs ; null s'il en manque un. */
export function ecart(actuel, avant) {
  if (actuel == null || avant == null) return null;
  return actuel - avant;
}

/** Un écart de taux en points : `+1,2 pt` ; null s'il manque un taux. */
export function points(actuel, avant) {
  const e = ecart(actuel, avant);
  if (e == null) return null;
  const p = arrondi(e * 1000) / 10;
  return p === 0 ? "= 0 pt" : (p < 0 ? moins : "+") + decimal(Math.abs(p), 1) + " pt";
}

/** Un montant signé : `+97 €`, `−12 €`. */
export function signe(v, format) {
  return (v < 0 ? moins : "+") + format(Math.abs(v));
}

function argent(v, devise) {
  return (Math.abs(v) >= 100 ? entier(v) : decimal(v, 2)) + " " + devise;
}

/** `7,10 €`, `1 240 €`. */
export function euros(v) {
  return argent(v, "€");
}

/** `7,10 $`, `1 240 $`. */
export function dollars(v) {
  return argent(v, "$");
}

/** `dont 4,10 € de voix`. */
export function dontVoix(v) {
  return "dont " + euros(v) + " de voix";
}

/** `1er`, `152e`. */
export function rang(p) {
  return p === 1 ? "1er" : p + "e";
}

// dates

function date(d) {
  return d instanceof Date ? d : new Date(d);
}

function df(options) {
  return new Intl.DateTimeFormat(langue, { timeZone: fuseau, ...options });
}

const fHeure = df({ hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
const fJourCourt = df({ day: "numeric", month: "short" });
const fDateLongue = df({ weekday: "long", day: "numeric", month: "long" });
const fJourSemaine = df({ weekday: "long" });
const fMoisAnnee = df({ month: "long", year: "numeric" });
const fCle = df({ year: "numeric", month: "2-digit", day: "2-digit" });

/** `14:52` */
export function heure(d) {
  return fHeure.format(date(d));
}

/** `17 nov.` */
export function jourCourt(d) {
  return fJourCourt.format(date(d));
}

/** `17 nov.` depuis une clé `yyyy-MM-dd`, la clé telle quelle si elle ne se lit pas. */
export function jourCourtCle(cle) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(cle);
  return m ? jourCourt(Date.UTC(+m[1], +m[2] - 1, +m[3], 12)) : cle;
}

/** `12 nov., 14:52` */
export function jourHeure(d) {
  return jourCourt(d) + ", " + heure(d);
}

/** `jeudi` */
export function jourSemaine(d) {
  return fJourSemaine.format(date(d));
}

/** `Jeudi 12 novembre` */
export function dateLongue(d) {
  const s = fDateLongue.format(date(d));
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** `mars 2027` */
export function moisAnnee(d) {
  return fMoisAnnee.format(date(d));
}

/** `à l'instant`, `il y a 3 min`, `il y a 2 h`, `il y a 4 j`. */
export function ilYa(d, maintenant = Date.now()) {
  const s = Math.max(0, (date(maintenant) - date(d)) / 1000);
  if (s < 60) return "à l'instant";
  if (s < 3600) return "il y a " + Math.floor(s / 60) + " min";
  if (s < 86_400) return "il y a " + Math.floor(s / 3600) + " h";
  return "il y a " + Math.floor(s / 86_400) + " j";
}

// jour calendaire de Paris, en jours depuis l'époque
function jourParis(d) {
  const p = Object.fromEntries(fCle.formatToParts(date(d)).map((x) => [x.type, x.value]));
  return Date.UTC(+p.year, +p.month - 1, +p.day) / 86_400_000;
}

/** Âge d'une vidéo en jours calendaires de Paris : `J+0` à `J+6`, puis `19 j`. */
export function age(d, maintenant = Date.now()) {
  const n = Math.max(0, jourParis(maintenant) - jourParis(d));
  return n < 7 ? "J+" + n : n + " j";
}

/** Cadence d'une source : `15 min`, `1 h`, `3 h`. */
export function cadence(s) {
  return s < 3600 ? entier(s / 60) + " min" : entier(s / 3600) + " h";
}

/** Une durée courte : `0,8 s`, `12 s`. */
export function duree(s) {
  return s < 10 ? decimal(s, 1) + " s" : entier(s) + " s";
}

/** Un temps cumulé : `45 s`, `12 min`, `3,4 h`, `164 h`. */
export function cumul(s) {
  if (s < 60) return duree(s);
  if (s < 3600) return entier(s / 60) + " min";
  if (s < 36_000) return decimal(s / 3600, 1) + " h";
  return entier(s / 3600) + " h";
}

/** Durée d'un passage, à la seconde : `< 1 s`, `12 s`. */
export function passage(s) {
  return s < 1 ? "< 1 s" : entier(s) + " s";
}

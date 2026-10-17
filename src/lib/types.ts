// Mirrors of the Rust structs (serde camelCase).

type N = number | null | undefined;
type T = string | null | undefined;

export interface DayRecord {
  date: string; // YYYY-MM-DD
  tempMax?: N; tempMaxTime?: T;
  tempMin?: N; tempMinTime?: T;
  tempAvg?: N;
  rain?: N; rainRateMax?: N; rainHourMax?: N;
  gustMax?: N; gustBearing?: N; gustTime?: T;
  windAvgMax?: N; windRun?: N; windDirDominant?: N;
  pressMin?: N; pressMax?: N;
  humMin?: N; humMax?: N;
  dewMin?: N; dewMax?: N;
  apparentMax?: N; apparentMin?: N;
  feelsMax?: N; feelsMin?: N;
  sunshine?: N; et?: N; solarMax?: N; uvMax?: N;
  hdd?: N; cdd?: N;
}

export type NumericDayField = {
  [K in keyof DayRecord]-?: DayRecord[K] extends N ? K : never;
}[keyof DayRecord];

export interface Units { temp: string; wind: string; press: string; rain: string }

export interface LiveReading {
  fetchedAt: number;
  stationTime?: T;
  temp?: N; tempHigh?: N; tempHighTime?: T; tempLow?: N; tempLowTime?: T; tempTrend?: N;
  feelsLike?: N; apparent?: N; windChill?: N; dewPoint?: N;
  humidity?: N; humHigh?: N; humLow?: N;
  pressure?: N; pressHigh?: N; pressLow?: N; pressTrend?: N; pressTrendText?: T;
  windAvg?: N; windLatest?: N; gustRecent?: N; gustHigh?: N; gustHighTime?: T;
  bearing?: N; bearingAvg?: N; bearingCompass?: T;
  rainToday?: N; rainRate?: N; rainRateHigh?: N; rainLastHour?: N; rainMonth?: N; rainYear?: N;
  solar?: N; solarHigh?: N; uv?: N; uvHigh?: N;
  forecast?: T;
  units: Units;
}

export interface Alerts {
  enabled: boolean;
  gustAbove: number | null;
  frostBelow: number | null;
  heatAbove: number | null;
  rainStart: boolean;
}

export interface Settings {
  stationName: string;
  stationUrl: string;
  username: string | null;
  password: string | null;
  dayfilePath: string | null;
  pollSeconds: number;
  theme: "system" | "light" | "dark";
  alerts: Alerts;
}

export interface Status {
  connected: boolean;
  lastOk: number | null;
  lastError: string | null;
  syncing: boolean;
  dayCount: number;
  lastSync: number | null;
}

/** metric -> [unixMs, value][] */
export type Samples = Record<string, [number, number][]>;

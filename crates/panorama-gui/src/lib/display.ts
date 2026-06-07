export type CurrentDisplay = {
  saved: boolean;
  screenMode: string;
  ratio: string;
  pane1Media: string[];
  pane2Media: string[];
  pane1PreviewSrc: string | null;
  pane2PreviewSrc: string | null;
  pane1Metrics: string[];
  pane2Metrics: string[];
  pane1Badges: string[];
  pane2Badges: string[];
  metricsColor: string;
  metricsAlign: string;
  metricsPosition: string;
  displayFilter: string | null;
  brightness: number;
};

export function emptyDisplay(): CurrentDisplay {
  return {
    saved: false,
    screenMode: "Full Screen",
    ratio: "2:1",
    pane1Media: [],
    pane2Media: [],
    pane1PreviewSrc: null,
    pane2PreviewSrc: null,
    pane1Metrics: [],
    pane2Metrics: [],
    pane1Badges: [],
    pane2Badges: [],
    metricsColor: "#FFFFFF",
    metricsAlign: "Left",
    metricsPosition: "Top",
    displayFilter: null,
    brightness: 75,
  };
}

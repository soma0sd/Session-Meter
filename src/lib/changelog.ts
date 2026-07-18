// App changelog shown in the "what's new" window, localized by the settings language and
// grouped by minor version. Used as a fallback when the GitHub fetch fails; keep in sync
// with CHANGELOG.md and CHANGELOG.en.md.
export interface ChangelogEntry {
  version: string;
  date: string;
  ko: string[];
  en: string[];
}

export const changelog: ChangelogEntry[] = [
  {
    version: "0.2",
    date: "2026-07-18",
    ko: [
      "자동 업데이트: 새 버전 자동 확인 + 위젯·트레이 메뉴에서 원클릭 업데이트.",
      "업데이트 소식 창: 변경 로그를 설정 언어에 따라 표시(GitHub 저장소에서 가져옴).",
      "통계 창에 계정 이름과 이메일 표시.",
      "트레이 더블클릭으로 통계를 열 때 위젯이 깜빡이거나 사라지던 문제 수정.",
      "첫 실행 시 위젯이 바로 표시되도록 함.",
    ],
    en: [
      "Auto-update: automatic new-version check with one-click update from the widget and tray menu.",
      "What's-new window: changelog shown per the language setting, fetched from the GitHub repository.",
      "Show the account name and email in the statistics window.",
      "Fixed the widget flickering or disappearing when a tray double-click opened the stats window.",
      "Show the widget immediately on first run.",
    ],
  },
  {
    version: "0.1",
    date: "2026-07-18",
    ko: [
      "트레이 아이콘·데스크톱 위젯·통계 창으로 Claude 구독 사용량(5시간·주간)을 표시.",
      "커스텀 테마 메뉴, 임계값·초기화 알림, 다크/라이트/시스템 테마, 한국어·영어 UI.",
      "claude.ai 세션 기반 사용량 조회(Windows DPAPI 암호화), 시작 시 자동 실행.",
    ],
    en: [
      "Claude subscription usage (5-hour / weekly) via a tray icon, desktop widget, and stats window.",
      "Custom themed menu, threshold/reset notifications, dark/light/system themes, Korean/English UI.",
      "claude.ai session-based usage (Windows DPAPI-encrypted), launch at startup.",
    ],
  },
];

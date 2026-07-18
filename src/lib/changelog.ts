// App changelog shown in the "what's new" window, localized by the settings language.
// Keep in sync with CHANGELOG.md (Korean) and CHANGELOG.en.md (English).
export interface ChangelogEntry {
  version: string;
  date: string;
  ko: string[];
  en: string[];
}

export const changelog: ChangelogEntry[] = [
  {
    version: "0.2.1",
    date: "2026-07-18",
    ko: [
      "트레이 더블클릭으로 통계 창을 열 때 위젯이 사라지던 문제 수정.",
      "첫 실행 시 위젯이 바로 표시되도록 함.",
      "업데이트 소식을 GitHub 저장소에서 가져오도록 변경.",
    ],
    en: [
      "Fixed the widget disappearing when a tray double-click opened the statistics window.",
      "Show the widget immediately on first run.",
      "Fetch the what's-new changelog from the GitHub repository.",
    ],
  },
  {
    version: "0.2.0",
    date: "2026-07-18",
    ko: [
      "새 버전 자동 확인 및 위젯·트레이 메뉴에서 원클릭 업데이트.",
      "업데이트 소식(변경 로그) 창 추가 - 설정 언어에 따라 한국어/영어로 표시.",
      "통계 창에 계정 이름과 이메일 표시.",
      "통계 창을 열 때 위젯이 깜빡이던 문제 수정.",
    ],
    en: [
      "Automatic new-version check with one-click update from the widget and tray menu.",
      "Added a what's-new (changelog) window, shown in Korean or English per the language setting.",
      "Show the account name and email in the statistics window.",
      "Fixed the widget flickering when opening the statistics window.",
    ],
  },
  {
    version: "0.1.0",
    date: "2026-07-18",
    ko: [
      "트레이 아이콘·데스크톱 위젯·통계 창으로 Claude 구독 사용량(5시간·주간)을 표시.",
      "커스텀 테마 메뉴, 임계값·초기화 알림, 다크/라이트/시스템 테마, 한국어·영어 UI.",
      "claude.ai 세션 기반 사용량 조회(Windows DPAPI 암호화), 시작 시 자동 실행.",
      "GitHub 릴리스 기반 자동 업데이트(서명 검증).",
    ],
    en: [
      "Claude subscription usage (5-hour / weekly) via a tray icon, desktop widget, and stats window.",
      "Custom themed menu, threshold/reset notifications, dark/light/system themes, Korean/English UI.",
      "claude.ai session-based usage (Windows DPAPI-encrypted), launch at startup.",
      "GitHub-based auto-update with signature verification.",
    ],
  },
];

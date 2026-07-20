<h1 align="center">SessionMeter</h1>

<p align="center">
  Claude 구독 사용량(5시간 세션과 주간 한도)을 시스템 트레이 아이콘·데스크톱 위젯·통계 창으로
  한눈에 관리하는 데스크톱 앱입니다.
  <br />
  <a href="https://v2.tauri.app/">Tauri v2</a> + Svelte로 제작했으며 MIT 라이선스로 배포합니다.
</p>

---

> [!WARNING]
> **비공식 도구이며 이용약관에 유의해야 합니다.**
> SessionMeter는 로그인된 본인 claude.ai 세션을 이용해 claude.ai 사용량 페이지와 동일한 데이터를
> 가져옵니다. 공식/공개 API가 아니며 Anthropic API 키를 사용하지 않습니다.
> Anthropic 소비자 약관은 봇·스크립트 등 자동 수단을 통한 서비스 접근과 데이터 수집을 제한하므로,
> 이 방식은 약관에 어긋날 수 있고 계정 제재 위험이 있습니다.
> 반드시 [Anthropic 소비자 약관](https://www.anthropic.com/legal/consumer-terms)을 확인하고
> **본인 책임**으로 사용하십시오. 본 프로젝트는 Anthropic과 무관하며, "Claude"는 Anthropic, PBC의
> 상표입니다. SessionMeter는 해당 상표를 제품명으로 사용하지 않으며, 호환 대상을 설명하기 위해서만
> "Claude"를 언급합니다.
>
> **Gemini는 훨씬 더 비공식적·실험적입니다.** 공식 API가 없어, 별도 프로세스로 띄운
> 내장 로그인 창에서 본인 Google 계정으로 로그인한 뒤 gemini.google.com/usage 페이지의 사용량 표시를
> 그대로 읽어옵니다(화면 스크래핑). Google은 임베드 브라우저의 로그인을 정책적으로 차단하므로, 이 창은
> 일반 브라우저처럼 보이도록 위장한 상태로 동작합니다. Google이 이를 다시 차단하면 로그인이 실패할 수
> 있으며, 페이지 구조가 바뀌면 수치가 잘못 표시되거나 조회가 실패할 수 있습니다. 반드시
> [Google 서비스 약관](https://policies.google.com/terms)을 확인하고 **본인 책임**으로 사용하십시오.
> "Gemini"는 Google LLC의 상표이며 본 프로젝트는 Google과 무관합니다.

## 기능

- **트레이 아이콘**: 5시간 세션의 남은 사용량을 색상(녹색/노랑/빨강)으로 표시하고 테마를 따릅니다.
- **데스크톱 위젯**: 테두리 없는 항상 위(토글) 창으로 작업 표시줄에 나타나지 않습니다. 5시간·주간
  사용량과 초기화까지 남은 시간을 보여 주며, 항상 위·이동 잠금 토글과 통계·설정 바로가기 버튼이
  있습니다. 내용에 맞춰 창 높이가 자동 조절됩니다.
- **통계 창**(트레이 더블클릭): 전체 사용 항목, 이력 차트, 소진 예측, 초기화 일정.
- **커스텀 테마 메뉴**(트레이 우클릭): 위젯 표시/숨김, 통계, 설정, 종료. 사용량은 주기적으로 자동
  갱신되므로 수동 새로고침 항목은 없습니다.
- **알림**: 사용량 80%·95% 도달 및 한도 초기화 시 데스크톱 알림.
- **다크/라이트/시스템 테마**: 트레이·위젯·통계·설정·메뉴에 일관 적용.
- **다국어 UI**(한국어·영어): OS 언어 자동 감지, 설정에서 변경 가능.
- **시작 시 자동 실행**과 단일 인스턴스 동작.
- 조절 가능한 **위젯 투명도**.

## 트레이 조작

| 동작 | 결과 |
| --- | --- |
| 좌클릭 | 위젯 표시 / 숨김 |
| 더블클릭 | 통계 창 열기 |
| 우클릭 | 테마 메뉴 열기 |

## 요구 사항

- Windows 10/11(주 대상, macOS/Linux는 best-effort).
- [WebView2 런타임](https://developer.microsoft.com/microsoft-edge/webview2/)(최신 Windows에는 기본 포함).

## 설치

개발 환경 없이 바로 사용하려면 미리 빌드된 Windows 설치 프로그램을 내려받으십시오.

1. **[최신 릴리스 페이지](https://github.com/soma0sd/Session-Meter/releases/latest)** 로 이동합니다.
2. **Assets** 목록에서 `SessionMeter_x.y.z_x64-setup.exe`(NSIS 설치 프로그램)를 내려받습니다
   (`x.y.z`는 버전 번호).
3. 내려받은 파일을 실행해 설치합니다. 서명되지 않은 빌드라 Windows SmartScreen 경고가 뜨면
   **추가 정보 → 실행**을 눌러 진행합니다.
4. 설치가 끝나면 앱이 시스템 트레이에만 나타납니다(작업 표시줄에 창이 뜨지 않음). 트레이 아이콘을
   **좌클릭**하면 위젯이, **우클릭**하면 메뉴가 열립니다.
5. 위젯 또는 설정 창에서 claude.ai(선택적으로 Gemini)에 로그인하면 사용량이 표시됩니다.

새 버전이 나오면 앱이 자동으로 확인해 위젯·트레이 메뉴에 설치 버튼을 띄웁니다. 버튼을 누르면
내려받아 설치하고 재실행합니다(자동 설치는 아님).

## 개발

사전 준비: [Rust](https://rustup.rs/), [Node.js](https://nodejs.org/) 20+,
[Tauri v2 사전 요구 사항](https://v2.tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev      # 개발 모드 실행
npm run tauri build    # 릴리스 빌드 + 설치 프로그램(NSIS) 생성
```

프런트엔드 전용: `npm run build`(번들), `npm run check`(svelte-check).

## 문서

사용 설명서는 [`docs/`](docs/)에 있으며 GitHub Pages로 배포됩니다(한국어).
가져올 수 있는 데이터의 범위와 한계는 [`docs/DATA_CAPABILITIES.md`](docs/DATA_CAPABILITIES.md)를
참고하십시오.

## 보안·프라이버시

- **세션은 기기 밖으로 나가지 않습니다.** SessionMeter는 로그인 창에서 얻은 claude.ai 세션 쿠키를
  사용자별 파일(`session.dat`)로 OS 애플리케이션 데이터 폴더에 저장합니다. `claude.ai`로만 HTTPS
  전송되며, 저장소에 커밋되거나 프로젝트로 전송되지 않습니다.
- **Windows에서는 저장 시 암호화됩니다.** `session.dat`는 Windows 데이터 보호 API(DPAPI, 사용자
  범위)로 암호화되어 다른 사용자 계정·오프라인 디스크·복사된 백업에서 복호화할 수 없습니다.
  macOS/Linux(best-effort)에서는 아직 사용자 범위 평문 파일이며 OS 시크릿 서비스 연동은 예정입니다.
  어느 경우든 로그인 상태에서 같은 사용자로 실행 중인 프로세스는 세션을 읽을 수 있는데, 이는 브라우저가
  자체 쿠키를 보관하는 방식과 같습니다. 로그아웃하면 `session.dat`가 삭제됩니다.
- **텔레메트리 없음.** `claude.ai` 외의 네트워크 통신은 하지 않습니다.
- 보안 문제 신고는 [`SECURITY.md`](SECURITY.md)를 참고하십시오.

## 로드맵

- macOS/Linux에서 저장 세션의 OS 시크릿 서비스 연동(Windows는 이미 DPAPI 암호화).
- 공식 Admin API 패널(개발자 API 토큰 사용량·비용(USD); Admin API 키 필요).
- 다중 계정.
- 사용 이력 CSV 내보내기.
- 위젯 그리드 도킹: 여러 위젯을 그리드로 붙여 배치·고정하고, 붙인 상태로 함께 이동.

## 라이선스

- 애플리케이션 코드: **MIT**([`LICENSE`](LICENSE) 참고).
- 번들 폰트(Noto Sans, Noto Sans KR): **SIL Open Font License 1.1**
  ([`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) 참고).

## 면책 및 상표 고지

- SessionMeter는 비공식 claude.ai 엔드포인트에 의존하므로 예고 없이 동작이 바뀔 수 있습니다.
  본인 계정의 세션으로만 동작하며, 사용 전 [Anthropic 약관](https://www.anthropic.com/legal/consumer-terms)을
  확인하고 본인 책임으로 사용하십시오.
- 본 프로젝트는 Anthropic과 제휴·후원·보증 관계가 없습니다. "Claude"와 "Anthropic"은 Anthropic,
  PBC의 상표이며, 호환 대상을 설명하기 위해서만 사용되었습니다.

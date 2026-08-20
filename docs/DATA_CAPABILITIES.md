# 데이터 능력

SessionMeter가 서비스별 사용량에 대해 얻을 수 있는 정보, 범위 밖 정보, 세션 처리 방식을 정리합니다.

## 서비스별 데이터 출처

| 서비스 | 출처와 인증 | 지원 범위 |
| --- | --- | --- |
| Claude | `claude.ai` 비공개 웹 API와 앱 내 로그인 창에서 확보한 브라우저 세션 쿠키 | 5시간·주간 및 응답에서 발견되는 사용량 버킷 |
| Codex | `chatgpt.com` 비공개 사용량 엔드포인트와 전용 격리 프로세스의 ChatGPT 로그인 세션 쿠키 | `limit_window_seconds`가 `604800`인 Codex 주간 한도만 |
| Gemini | `gemini.google.com/usage` 화면 스크래핑과 별도 프로세스 로그인 창 | 화면에 표시되는 구독 사용량, 실험적 기능 |
| Antigravity IDE | 실행 중인 IDE의 비공개 로컬 loopback API | Gemini 및 Claude/GPT 모델군의 코딩 쿼터, Windows 전용 |

Claude·Codex·Gemini의 연동은 공식 또는 공개 API가 아닙니다. Antigravity IDE 연동도 문서화되지 않은
로컬 인터페이스입니다. 서비스 정책이나 응답 구조가 바뀌면 로그인 또는 조회가 실패할 수 있습니다.

## Claude 구독 사용량

Claude 요청 흐름:

1. `GET https://claude.ai/api/organizations`으로 첫 조직의 `uuid`와 이름 조회
2. `GET https://claude.ai/api/organizations/{uuid}/usage`으로 사용량 조회
3. `GET https://claude.ai/api/account`으로 표시명과 이메일 조회

`claude.ai` 전체 쿠키 문자열과 데스크톱 User-Agent로 요청하며, `401/403`은 세션 만료로 처리합니다.
응답은 고정 스키마가 아닌 `{utilization, resets_at}` 형태를 동적으로 파싱합니다.

| 필드 | 의미 | 사용처 |
| --- | --- | --- |
| `five_hour.utilization` / `resets_at` | 5시간 세션 사용률 %와 초기화 시각 | 트레이·위젯·통계 |
| `seven_day.utilization` / `resets_at` | 주간 사용률 %와 초기화 시각 | 위젯·통계 |
| 기타 `{utilization, resets_at}` | 예: 모델군별 주간 한도 | 추가 버킷 표시 |
| `organization_name`, 계정 표시명·이메일 | 계정 라벨 | 설정 계정 패널 |

## Codex 구독 사용량

Codex 요청 흐름:

1. 앱 본체와 분리된 전용 프로세스의 `https://chatgpt.com/auth/login` 창에서 본인 ChatGPT 계정 로그인
2. `GET https://chatgpt.com/backend-api/wham/usage`으로 구독 한도 조회

`chatgpt.com` 세션 쿠키와 `OAI-App-Brand: codex` 요청 헤더를 사용합니다. 응답의 `401/403`은
세션 만료로 처리하며, 새 로그인 전까지 기존 정상 스냅샷을 보존합니다. 이 엔드포인트와 쿠키 형식은
공개 계약이 아닙니다.

로그인 창은 Gemini와 별개인 전용 WebView2 프로세스에서 실행합니다. ChatGPT의 Cloudflare 검사나
WebView2 렌더링 정지가 발생해도 앱 본체의 트레이·설정·위젯은 영향을 받지 않습니다. 부모 프로세스는
전용 파이프를 통해 받은 쿠키를 사용량 엔드포인트로 검증한 뒤에만 DPAPI 저장을 수행합니다.

| 필드 | 의미 | 사용처 |
| --- | --- | --- |
| `rate_limit.primary_window` / `secondary_window`의 `used_percent` / `reset_at` | `limit_window_seconds`가 `604800`인 창의 Codex 주간 사용률 %와 초기화 Unix 시각 | 트레이·위젯·통계 |
| `rate_limit.primary_window` / `secondary_window`의 `limit_window_seconds` | 한도 창 길이 | `604800`초 주간 창만 선택 |
| `plan_type` | 응답이 제공하는 ChatGPT 플랜 식별자 | 설정 계정 패널 |

Codex 표시 값은 `primary_window`와 `secondary_window` 중 `limit_window_seconds`가 `604800`인 주간 창만
사용합니다. 서버가 반환한 사용률을 0부터 100까지 정규화해 남은 사용량 `%`와 초기화까지 남은 시간으로
변환합니다. 5시간 창과 그 밖의 추가 창은 표시하거나 이력에 저장하지 않습니다.

## 공통 계산과 범위

- **남은 사용량 %**: 버킷별 `max(0, 100 - utilization)`
- **초기화까지 남은 시간**: 서비스 응답의 초기화 시각 기준 실시간 카운트다운
- **로컬 사용 이력·소진 예측·알림**: 폴링 표본과 설정 기준으로 앱에서 계산
- **Codex 범위 제외**: 5시간 세션, OpenAI API 토큰 사용량·비용·조직 사용량, 별도 모델별 한도 및 `604800`초가 아닌 추가 한도 창
- **Claude 범위 제외**: Anthropic Admin API의 개발자 API 토큰 사용량·USD 비용, 원시 토큰 수와 내부 한도값
- **공식 API 대체 아님**: 각 서비스의 구독 세션 한도와 개발자 API 사용량은 서로 다른 지표

## 세션 저장·보안

브라우저 로그인 서비스의 쿠키는 설정 파일과 분리된 OS 애플리케이션 데이터 폴더에 서비스별로 저장됩니다.
Claude는 기존 세션 호환성을 위해 `session.dat`, Codex는 `session.codex.dat`, Gemini는
`session.gemini.dat`를 사용합니다. 쿠키는 각 서비스의 HTTPS 요청에만 전송되며, Antigravity IDE는
로컬 loopback API만 사용하므로 브라우저 세션을 저장하지 않습니다.

- **Windows 암호화**: 모든 브라우저 세션 파일을 사용자 범위 Windows DPAPI로 보호. 다른 사용자·오프라인
  디스크·복사된 백업에서는 복호화 불가
- **기타 플랫폼**: 현재 사용자 범위 평문 파일. OS 시크릿 서비스 연동 예정
- **같은 사용자 주의**: 로그인 상태에서 같은 사용자로 실행 중인 프로세스는 활성 세션에 접근 가능. 브라우저
  쿠키 저장 방식과 같은 보안 경계
- **세션 수명**: 서비스별 로그아웃 시 해당 파일 삭제. 릴리스 빌드는 원시 API 응답을 디스크에 저장하지 않음

로그인·사용량 조회·업데이트 확인을 위한 서비스 및 배포 서버 통신 외에 사용 데이터를 수집하거나
전송하지 않습니다. 사용 전 해당 서비스의 이용약관을 확인하고 본인 계정 또는 본인 PC의 IDE로만
사용해야 합니다.

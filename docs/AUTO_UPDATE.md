# 자동 업데이트 설정

SessionMeter는 Tauri v2 업데이터로 GitHub 릴리스 기반 자동 업데이트를 지원합니다. 코드와 설정은
저장소에 포함되어 있으며, 실제로 활성화하려면 아래 준비가 필요합니다.

## 구성 요약

- **서명 공개키**: `src-tauri/tauri.conf.json`의 `plugins.updater.pubkey`에 임베드되어 있습니다.
- **엔드포인트**: `https://github.com/soma0sd/Session-Meter/releases/latest/download/latest.json`
  (`plugins.updater.endpoints`). 저장소 경로가 다르면 이 값을 수정하십시오.
- **동작**: 앱은 시작 시와 이후 10분마다 자동으로 최신 릴리스를 확인하고(`update://available` 이벤트), **설정 >
  업데이트**에서 수동으로 확인·설치할 수 있습니다.
- **아티팩트**: `bundle.createUpdaterArtifacts: true`로 빌드하면 설치본과 함께 `.sig` 서명,
  `latest.json` 매니페스트가 생성됩니다.

## 서명 키

- 개인키: `~/.tauri/sessionmeter.key` (암호 없음). **비밀로 유지**하고 저장소에 커밋하지 마십시오.
- 공개키(`~/.tauri/sessionmeter.key.pub`)는 이미 앱에 임베드되어 있습니다.
- 개인키를 분실하면 이후 업데이트에 서명할 수 없으므로 안전하게 백업하십시오.

## 활성화 절차

1. GitHub 저장소(`soma0sd/Session-Meter`)를 생성하고 코드를 push합니다.
2. 저장소 **Secrets**에 다음을 추가합니다.
   - `TAURI_SIGNING_PRIVATE_KEY`: `~/.tauri/sessionmeter.key` 파일의 내용
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: 빈 값(키에 암호가 없음)
3. 버전 태그(`vX.Y.Z`)를 push하면 GitHub Actions가 서명된 설치본 + `.sig` + `latest.json`을 릴리스
   초안에 업로드합니다.
4. **릴리스를 게시(publish)**합니다. 업데이터의 `releases/latest`는 게시된 릴리스만 인식하므로,
   초안 상태로는 업데이트가 배포되지 않습니다.

## 동작 흐름

1. 설치된 앱이 시작 시와 이후 10분마다 `latest.json`을 확인합니다.
2. 앱 버전보다 높은 서명 릴리스가 있으면 설정 > 업데이트에 새 버전이 표시됩니다.
3. **지금 설치**를 누르면 서명을 검증한 뒤 설치본을 내려받아 설치하고 앱을 재시작합니다.

## 주의

- **이 빌드(updater 포함)부터** 이후의 게시 릴리스를 인식합니다. updater가 없던 이전 빌드는 자동
  갱신되지 않습니다.
- 저장소가 **비공개**이면 업데이터가 릴리스 자산에 접근할 토큰이 필요합니다(공개 저장소는 불필요).
- 배포 자체의 이용약관 리스크(비공식 claude.ai API 사용)는 자동 업데이트와 무관하게 별도로
  존재합니다. [`README`](../README.md)의 고지를 확인하십시오.

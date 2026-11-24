# Microsoft Store용 MSIX

Partner Center의 제품 ID 화면에 나온 패키지 이름, 게시자, 게시자 표시 이름으로 빌드합니다. Windows SDK x64의 MakeAppx 및 MakePri와 기존 Tauri 빌드 환경이 필요합니다.

```powershell
npm ci
.\store\build-msix.ps1 -IdentityName '<Package/Identity/Name>' -Publisher '<Package/Identity/Publisher>' -PublisherDisplayName '<PublisherDisplayName>'
```

`package/store/SessionMeter_X.Y.Z.0_x64.msix`는 인증 제출용 미서명 패키지입니다. 한국어 표시 이름은 **세션미터**, 영어 표시 이름은 **SessionMeter**입니다. Store판 업데이트는 Microsoft Store가 관리하고, 시작 앱 등록은 Windows StartupTask를 사용합니다.

제출 항목은 [등록 정보](LISTING.md)를 참고하십시오.

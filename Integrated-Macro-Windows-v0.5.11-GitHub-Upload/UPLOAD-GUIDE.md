# Windows 0.5.11 일회성 GitHub 빌드

이 묶음은 실행 파일이 아닌 GitHub 빌드 준비물입니다. 앱 소스는 기존 Mac 배포 0.5.11과 같고 미배포 수정은 포함하지 않습니다. 기존 Codex 계정의 GitHub 연결을 바꾸거나 인증정보를 전달할 필요가 없습니다. 본인 계정으로 로그인한 브라우저에서 아래 두 파일을 저장소에 넣습니다.

## 1. 빌드 ZIP 올리기

방금 만든 비공개 저장소를 엽니다. `Add file` → `Upload files`에서 다음 파일을 저장소 최상위에 올리고 기본 브랜치에 커밋합니다. 빈 저장소라 메뉴가 없으면 시작 화면의 기존 파일 업로드 링크를 사용합니다.

`Integrated-Macro-Windows-v0.5.11-Build-Kit.zip`

이 ZIP은 압축 해제하거나 이름을 바꾸지 마세요. 워크플로가 SHA-256을 확인한 뒤 빌드 서버에서 압축을 풉니다.

## 2. 워크플로 만들기

`Add file` → `Create new file`을 선택하고 파일 이름을 정확히 다음과 같이 입력합니다.

`.github/workflows/build-windows.yml`

함께 제공한 `workflow.yml.txt`를 텍스트 편집기로 열어 **전체 내용**을 붙여넣고 같은 브랜치에 커밋합니다. `.txt` 파일을 그대로 최상위에 업로드하는 방식으로는 실행되지 않습니다.

## 3. 결과 확인

저장소의 Actions가 활성화되어 있으면 워크플로를 저장한 커밋에서 `Build Windows 0.5.11`이 시작됩니다. `Actions` 탭에서 진행 상황을 확인합니다. 필요하면 `Run workflow`로 다시 요청할 수도 있습니다.

- 성공: 실행 페이지의 Artifacts에서 `windows-0.5.11-…`을 내려받습니다. 설치용 파일은 `-setup.exe`이며 `report.json`에 검사 결과와 파일 해시가 기록됩니다.
- 실패: 빨간색 단계의 로그를 복사해 이 대화로 전달하면 이어서 원인을 점검할 수 있습니다. 테스트 실패를 건너뛰는 옵션은 넣지 않았습니다.

비공개 저장소를 만들었다고 이 Codex 연결에 읽기·쓰기 권한이 자동으로 생기지는 않습니다. 여기서 실행 상태나 로그를 직접 볼 수 있다고 가정하지 않습니다. 이번 준비 과정에서는 GitHub 업로드·빌드 실행을 하지 않았고 Windows 설치·앱 실행·실제 예매 검증도 하지 않았습니다.

소스와 빌드 로그는 GitHub에 남습니다. 필요한 설치 파일과 결과를 내려받은 뒤 저장소 보관·삭제는 본인 계정에서 결정하면 됩니다.

GitHub 공식 안내: [파일 업로드](https://docs.github.com/en/repositories/working-with-files/managing-files/adding-a-file-to-a-repository), [새 파일 만들기](https://docs.github.com/en/repositories/working-with-files/managing-files/creating-new-files).

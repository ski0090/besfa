# Feature-Sliced Design

Besfa의 Flutter 에디터 UI는 Feature-Sliced Design(FSD)의 레이어와 의존성 규칙을 Flutter에 맞춰 적용한다.

FSD는 기능 중심으로 UI 코드를 구성해, 기능 추가와 리팩터링이 서로에게 미치는 영향을 제한하는 구조다.

## 적용 원칙

처음부터 모든 레이어를 만들지 않는다. 실제 코드가 필요해지는 시점에 해당 레이어와 slice를 추가한다.

```text
lib/
├─ app/                        # 앱 전체 초기화와 조립
├─ pages/                      # 전체 화면 단위
├─ widgets/                    # 여러 feature/entity를 조합한 큰 UI
├─ features/                   # 사용자가 실행하는 독립 액션
├─ entities/                   # 프로젝트, 씬, 에셋 등의 도메인 표현
└─ shared/                     # 재사용 가능한 UI와 플랫폼 연결
```

## 레이어 책임

| 레이어 | 책임 | 예시 |
| --- | --- | --- |
| `app` | 전역 초기화와 의존성 조립 | 테마, 창 설정, 라우팅 |
| `pages` | 한 화면 전체를 구성 | 프로젝트 허브, 씬 에디터 |
| `widgets` | 여러 기능을 조합한 큰 화면 영역 | 최근 프로젝트 목록, 인스펙터 패널 |
| `features` | 사용자가 수행하는 액션 | 프로젝트 생성, 프로젝트 열기, 씬 저장 |
| `entities` | 에디터가 다루는 도메인 데이터 | 프로젝트 정보, 씬 정보, 에셋 정보 |
| `shared` | 특정 도메인에 묶이지 않은 재사용 코드 | 공통 버튼, FFI 어댑터, 경로 유틸 |

## 의존성 규칙

상위 레이어는 자신보다 낮은 **모든 레이어**를 직접 import할 수 있다. 중간 레이어를 반드시 거칠 필요는 없다.

```text
pages → widgets, features, entities, shared
widgets → features, entities, shared
features → entities, shared
entities → shared
app → 모든 하위 레이어를 조립할 수 있음
```

따라서 아래 의존성은 올바르다.

```text
pages/project_hub → features/create_project
pages/project_hub → entities/project
```

같은 레이어 안의 다른 slice는 직접 import하지 않는다. 여러 feature를 조합해야 하면 `pages` 또는 `widgets` 레이어에서 조합한다.

```text
피해야 함:
features/create_project → features/open_project
```

## Slice 내부 구조

기능이나 엔터티가 커지면 slice 안을 역할별 segment로 나눈다.

```text
features/create_project/
├─ ui/                         # 다이얼로그, 폼, 버튼
├─ model/                      # 입력 상태, 검증, 생성 요청
└─ lib/                        # 해당 feature 내부 보조 코드
```

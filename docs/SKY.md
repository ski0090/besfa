# 하늘 편집 기획

씬의 하늘을 Inspector에서 고치고 씬 파일에 저장한다. 하늘은 `besfa_editor_plugin`이 제공하는 Reflect 컴포넌트 하나로, 게임 코드가 실행 중에 값을 써도 화면이 따라간다(예: Duskhold의 하루 시계가 시각에 따라 하늘색을 바꾼다).

## 목표

- 하늘은 씬 엔티티의 컴포넌트 `besfa_editor_plugin::sky::Sky`다. 다른 컴포넌트처럼 `.scn.ron`에 저장되고, Inspector·되돌리기·`besfa_mcp`의 `set`이 그대로 통한다.
- 게임 코드는 `Query<&mut Sky>`로 값을 바꾼다. 플러그인이 바뀐 값을 매 프레임 화면에 반영하므로 게임은 렌더링을 몰라도 된다. 편집 상태와 실행 상태, 에디터 없이 실행한 게임 모두 같다.
- 하늘은 Inspector에서 고친다. 색이나 이미지를 고치면 편집 상태의 뷰포트에 바로 보이고, Save scene이 씬에 남긴다. 전용 Sky 탭은 두었다가 지웠다: 하늘을 게임이 정하는 경우(Duskhold의 하루 시계)에는 Inspector와 하는 일이 같았다.
- Bevy 0.20 밖의 새 크레이트를 더하지 않는다.

## 핵심 결정: 그라데이션과 큐브맵 둘 다, 하나의 경로로

| 방식 | 장점 | 단점 |
| --- | --- | --- |
| 그라데이션(절차적) | 색 세 개라 게임이 시각에 따라 보간하기 쉽다. 에셋이 필요 없다 | 구름·별 같은 무늬가 없다 |
| 큐브맵 이미지 | 그린 하늘·HDR 사진을 그대로 쓴다 | 하루 시계로 색을 바꾸기 어렵다(밝기와 회전만) |

둘 다 지원한다. 두 방식 모두 Bevy 0.20의 `Skybox`(카메라 컴포넌트, 큐브맵 이미지)로 그린다. 그라데이션은 플러그인이 CPU에서 큐브맵(면마다 256×256)을 칠해 같은 `Skybox`에 넣으므로, 셰이더를 새로 쓰지 않고 무한히 먼 배경·깊이 처리·노출을 Bevy가 맡는다. 해와 달 원반도 이 큐브맵에 함께 칠한다. 값이 바뀐 프레임에만 다시 칠한다(약 39만 픽셀, 1.5 MB. 하루 시계가 매 프레임 해를 옮겨도 한 프레임에 몇 ms다). 면마다 256이면 1도가 약 2.8픽셀이라 지름 3도 원반이 8픽셀쯤으로, 샘플러의 선형 보간과 원반 가장자리의 부드러운 경계로 계단이 보이지 않는다.

절차적 대기 산란(Bevy의 `Atmosphere`)은 해 방향으로 하늘색이 정해져 "하늘색을 직접 고른다"는 요구와 맞지 않고, 카메라마다 무거운 설정이 필요해 이번에는 하지 않는다.

## 컴포넌트

```rust
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
#[reflect(Component, Default)]
pub struct Sky {
    pub mode: SkyMode,        // Gradient | Cubemap
    pub zenith: Color,        // 머리 위
    pub horizon: Color,       // 지평선
    pub ground: Color,        // 지평선 아래
    pub blend: f32,           // 지평선 색이 위로 퍼지는 정도(지수, 작을수록 천정 색이 넓다)
    pub cubemap: String,      // 에셋 폴더 기준 이미지 경로
    pub brightness: f32,      // 1이면 고른 색 그대로(카메라 노출을 상쇄한다)
    pub rotation: f32,        // 하늘을 위에서 보아 반시계로 돌리는 각도(도)
    pub sun: Sun,
    pub moon: Moon,
}

pub struct Sun {
    pub show: bool,           // 기본 true
    pub direction: Vec3,      // 보는 사람에게서 해 쪽으로(월드 기준, 길이는 상관없다)
    pub size: f32,            // 겉보기 지름(도), 기본 3
    pub color: Color,         // 원반과 둘레 빛의 색
    pub flare: f32,           // 렌즈 플레어 세기, 0이면 없음(기본) ~ 1
}

pub struct Moon {
    pub show: bool,           // 기본 false
    pub direction: Vec3,
    pub size: f32,            // 기본 4
    pub color: Color,         // 밝은 쪽의 색
    pub phase: f32,           // 0 삭, 0.25 상현(오른쪽 반), 0.5 보름, 0.75 하현(왼쪽 반), 1 다시 삭
}
```

- 모드를 바꿔도 다른 모드의 값은 남는다. 게임이 모드를 바꿔 낮은 그라데이션, 밤은 별 큐브맵처럼 쓸 수도 있다.
- 그라데이션 색: 방향의 높이 `y`(-1~1)가 0 이상이면 `horizon`에서 `zenith`로 `y^blend`만큼, 0 아래면 `horizon`에서 `ground`로 `min(1, -4y)`만큼 선형 색 공간에서 섞는다.
- 큐브맵: Bevy가 읽는 큐브맵(`.ktx2`, `.dds`)이나 여섯 면(+X, -X, +Y, -Y, +Z, -Z)을 세로로 쌓은 PNG·JPG(높이 = 너비 × 6). 쌓은 이미지는 플러그인이 큐브맵으로 바꿔 읽는다. 아직 읽는 중이거나 읽지 못하면 그라데이션을 보이고, 실패는 로그에 남긴다.
- 해: 원반은 `color`로 칠하고, 둘레로 지름의 네 배까지 옅어지는 빛을 더한다. 방향은 월드 기준이라 `rotation`과 상관없이 그 방향에 보인다. 지평선 아래(땅 색 부분)에서는 잘린다.
- 달: 원반 안에서 위상으로 정한 쪽만 `color`로 칠하고 어두운 쪽은 하늘이 비친다. 위상은 0~1을 한 바퀴로 보고, 원반을 공으로 여겨 빛이 `phase`에 따라 뒤(삭)에서 오른쪽(상현), 앞(보름), 왼쪽(하현)으로 돈다고 보고 경계를 부드럽게 그린다. 북반구에서 보듯 차는 달은 오른쪽이 밝다. 해와 마찬가지로 지평선 아래에서 잘린다.
- 렌즈 플레어: 화면 공간 효과라 큐브맵에 칠할 수 없으므로, 하늘을 그리는 카메라마다 UI 원 일곱 개(`Ghost`, `EditorOnly`라 Hierarchy와 씬 파일에 없다)를 그 카메라에 붙여(`UiTargetCamera`) 해가 보이는 화면 점에서 화면 가운데를 지나는 선 위에 늘어놓는다. 해 색에 `flare`만큼의 투명도로, 해가 화면 가장자리로 갈수록 옅어지고 화면 밖, 지평선 아래, 카메라 뒤에 있거나 카메라에서 해 쪽으로 쏜 광선이 메시에 맞으면(해가 가려지면) 숨는다. 게임 UI 아래(`GlobalZIndex(-1000)`)에 그리고 마우스를 받지 않는다. 편집 상태의 뷰포트(편집용 카메라)에서도 보인다.
- 해와 달은 그라데이션 모드에서만 그린다(플레어도). 큐브맵 모드에서는 이미지가 하늘 전체이므로 그리지 않는다.
- 게임은 빛의 방향을 그대로 쓸 수 있다: `DirectionalLight`는 Transform의 앞(-Z)으로 비추므로 해 방향은 `transform.back()`이다.
- 씬에 `Sky`가 여럿이면 처음 찾은 것을 쓴다. `Sky`가 없으면 플러그인은 아무것도 하지 않는다(지금처럼 `ClearColor`).

## 플러그인 동작

- `SkyPlugin`은 편집 모드와 상관없이 항상 들어간다(`default_plugins`).
- `Sky`가 바뀌거나 카메라가 새로 생기면 모든 3D 카메라에 `Skybox`를 넣는다. 밝기는 `brightness ÷ 카메라 노출`이라 노출이 다른 편집용 카메라와 게임 카메라에서 같은 색으로 보인다. 플러그인이 쓰는 미리보기·썸네일 카메라와, 게임이 직접 `Skybox`를 붙인 카메라는 건드리지 않는다.
- 플러그인이 넣은 `Skybox`는 표식(`SkyApplied`)과 함께 씬 파일에서 뺀다. `Sky`를 떼면 넣은 `Skybox`도 뗀다.
- Hierarchy의 + 메뉴의 Sky가 `spawn`의 `kind: sky`로 `Name("Sky")`와 기본 `Sky`를 가진 엔티티를 만든다.

## 프로토콜(버전 10)

- `SpawnKind::Sky`.
- `EntityInfo`에 `sky: bool`(기본 false, 거짓이면 쓰지 않는다): `besfa_mcp`의 `entities`가 하늘 엔티티를 알린다.
- 값 고치기는 새 명령 없이 기존 `set`을 쓴다.

## 게임에서 쓰기

```rust
use besfa_editor_plugin::Sky;

fn day_sky(
    clock: Res<DayClock>,
    daylight: Single<&Transform, With<DirectionalLight>>,
    mut skies: Query<&mut Sky>,
) {
    for mut sky in &mut skies {
        sky.zenith = clock.zenith_color();
        sky.horizon = clock.horizon_color();
        // 빛이 비추는 반대쪽이 해다.
        sky.sun.direction = *daylight.back();
        sky.sun.show = clock.is_day();
        sky.moon.direction = *daylight.back();
        sky.moon.show = !clock.is_day();
        sky.moon.phase = (clock.day() as f32 / 8.0).fract();
    }
}
```

## 하지 않는 것(후속)

- 하늘이 장면 조명(환경광, `EnvironmentMapLight`)에 주는 영향. 지금은 배경만 그린다.
- 구름, 별 같은 절차적 무늬. 큐브맵 모드 위의 해·달.
- HDR(`.hdr`/`.exr`) 정거 원통 이미지를 큐브맵으로 바꾸기.

## 테스트

- 플러그인: 렌즈 플레어가 하늘을 그리는 카메라마다 한 벌 생기고 하늘이 없어지면 사라지는지, 화면 가장자리에서 옅어지는지, 그라데이션 색 계산, 해 원반과 숨기기, 달 위상(삭·보름·상현), 회전해도 해가 월드 방향에 남는지, 큐브맵 이미지 생성, 카메라에 `Skybox`가 들어가고 값을 바꾸면 따라가며 `Sky`를 떼면 빠지는지, 씬 저장·읽기 왕복에서 `Sky`가 남고 `Skybox`는 빠지는지.
- 프로토콜: `SpawnKind::Sky`, `EntityInfo.sky`의 직렬화와 옛 보고와의 호환.

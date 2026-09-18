# AeroForge

[English](README.md) | **한국어**

AeroForge는 빠른 대화형 미리보기 경로와, SU2를 기반으로 한 별도의 Accurate 워크플로를 갖춘 네이티브 Bevy + egui 3D 공기역학 워크벤치입니다.

## 현재 기반 기능

- Box / Sphere / Cylinder 기본 형상, 피킹, 이동/회전/크기 조절 기즈모를 제공하는 네이티브 viewport-first 3D 편집기;
- fail-closed 경로/보안 규칙, 복구 및 감사(audit), 안정적인 `SceneObject.id` provenance, 공통 편집 기능을 갖춘 OBJ / STL / static glTF / GLB 삼각형 표면 가져오기;
- 독립적인 `aeroforge-flow-core` D3Q19 BGK CPU reference kernel;
- CPU/GPU parity smoke와 명시적인 device-limit 검사를 포함하는 실험적 네이티브 GPU D3Q19 WGSL compute 경로;
- periodic, wall/moving-wall, NEQ velocity/pressure, prescribed free-stream far-field 미리보기 경계 정책;
- 불안정하거나 고 Mach BGK 설정을 정량 CFD처럼 조용히 표시하지 않는 physical-scaling 진단;
- 생성된 configuration, 명시적인 SI coefficient reference/frame, 프로세스 실행, 구조화된 convergence/history 진단, aggregate/per-body force/moment 수집, cancellation ownership, persisted provenance를 포함하는 SU2 8.5.0 고정 Accurate adapter;
- deterministic built-in cell-center occupancy → Cartesian staircase tetrahedral Accurate reference 경로;
- 명시적 source admission, deterministic PLC/hole seed, parser/process provenance, positive-volume tetrahedral non-overlap, source/body normal 및 crease evidence, discrete triangulated normal-variation evidence, first-cell wall-normal height observation, 모든 tetrahedron의 6개 internal dihedral 전체 evidence, 모든 unique face의 centroid/normal orthogonality 전체 evidence, 모든 interior face의 adjacent-cell volume-ratio 및 face-centroid skewness 전체 evidence, triangulated source facet ↔ output body facet의 1:1 correspondence를 갖춘 선택적 user-installed external TetGen 경로;
- external TetGen provenance는 `aeroforge_tetgen_handoff.tsv` format v12로 저장되며, mesh fidelity는 근거 없이 승격하지 않고 명시적으로 미분류 상태를 유지합니다.

## 소스에서 실행

```bash
cargo run -p aeroforge-app --release
```

## Windows limited alpha

성공한 `AeroForge CI` 실행은 `AeroForge-Windows-x86_64-<source-commit>` 이름의 임시 Windows artifact를 만들며 14일 동안 보관합니다. 해당 GitHub Actions 실행을 열고 artifact를 내려받아 한 번 압축을 푼 뒤 `AeroForge.exe`를 직접 실행하면 됩니다. 이는 서명되지 않은 테스트 빌드이며 installer나 production release가 아닙니다.

패키지 구성:

- `AeroForge.exe` — 패키징된 데스크톱 애플리케이션;
- `README.txt` — runtime 요구사항과 startup-smoke가 실제로 보장하는 범위;
- `SHA256SUMS.txt` — `AeroForge.exe`의 SHA-256;
- `BUILD_COMMIT.txt` — 패키지를 빌드한 정확한 source-head commit;
- `CI_VALIDATION_COMMIT.txt` — workflow validation commit. Pull request 실행에서는 GitHub synthetic merge commit일 수 있지만 `BUILD_COMMIT.txt`는 정확한 PR head를 유지합니다.

패키지 job은 정확한 source head를 빌드하고, GitHub-hosted Windows runner에서 primary window를 숨긴 상태로 해당 release executable을 실행한 뒤, 최소 3개 rendered frame 이후 `AEROFORGE_STARTUP_SMOKE_OK`가 나타나야 artifact를 업로드합니다. Hosted Microsoft Basic Render Driver에서 정상 렌더링 이후 Bevy/wgpu teardown 도중 DX12 device가 유실되는 현상이 관찰됐기 때문에 smoke process는 이 경계에서 의도적으로 즉시 종료됩니다. 따라서 이 CI smoke가 입증하는 것은 제한된 startup/render evidence이며, **graceful GPU teardown이나 모든 실제 GPU/driver/display 환경과의 호환성을 보장하지 않습니다**.

Accurate SU2 실행에는 `SU2_RUN` 또는 `PATH`에서 찾을 수 있도록 별도로 설치한 SU2 8.5.0 runtime이 필요합니다. TetGen도 선택적 외부 설치 항목이며 external TetGen 경로를 사용할 경우 `TETGEN_EXECUTABLE` 또는 `PATH`를 설정합니다. 두 dependency 모두 Windows alpha artifact에 포함되지 않습니다.

## numerical / geometry / Accurate core 테스트

```bash
cargo test -p aeroforge-flow-core -p aeroforge-accurate-backend
```

## GPU smoke / parity check

```bash
cargo run -p aeroforge-gpu-smoke
```

GPU smoke는 앱이 실제 사용하는 동일한 WGSL을 실행하고 통제된 결과를 CPU reference와 비교합니다. Parity 통과는 테스트한 contract에서 구현 결과가 일치함을 입증할 뿐, aerodynamic accuracy를 검증하지 않습니다.

## 조작법

- 왼쪽 마우스: 카메라 orbit
- 오른쪽 마우스: pan
- 마우스 휠: zoom
- 왼쪽 Scene panel: geometry 및 wind source 생성/선택
- Viewport gizmo: 선택한 geometry 이동/회전/크기 조절
- 오른쪽 Inspector: transform, source parameter, simulation setting 편집
- Accurate workspace: `Viewport / Prepare / Run / Results`

## 정확도 / fidelity 정책

네이티브 D3Q19 경로는 **interactive preview solver**이며 검증된 high-fidelity CFD 대체재가 아닙니다. Poiseuille, Couette, cavity, open/far-field behavior, 통제된 cylinder study 같은 canonical regression도 각각 선언된 evidence 범위만 입증합니다.

내장 Accurate mesh는 deterministic staircase/voxel-derived 상태를 유지하며 **body-fitted가 아닙니다**.

선택적 external TetGen 경로는 더 강한 직접 source-surface 및 local tetrahedral-shape evidence를 갖습니다. Routine real-TetGen CI는 명시적인 수치 tolerance 안에서 input triangulated source facet과 output body-boundary facet의 1:1 coincidence를 확인하며, 528-triangle rounded fixture도 포함합니다. 또한 solver-bound 모든 tetrahedron의 6개 internal dihedral angle을 전부 평가하고, 모든 unique tetrahedral face에 대해 face-normal/centroid-connection orthogonality cosine을 평가하며, 명시적으로 제한된 정책 아래 모든 unique interior face의 adjacent-cell volume ratio와 face-centroid skewness를 모두 평가합니다.

Rounded real-TetGen fixture에서 관찰된 face-orthogonality evidence는 interior face 954개, boundary face 540개, 총 1,494개의 complete face test, minimum interior cosine `0.3927105399869913`, minimum boundary cosine `0.5161688582468765`입니다. 이는 fixture observation이지 engineering acceptance threshold가 아닙니다. Desktop policy는 interior 및 boundary cosine 모두에 의도적으로 느슨한 `1e-12` numerical floor와 20,000,000-face work budget을 사용합니다.

같은 fixture의 954개 interior face 전체 size-transition 평가에서는 maximum adjacent-cell volume ratio `108.24863139041692`가 관찰됐습니다. Desktop의 `1e12` maximum과 20,000,000-interior-face work budget은 solver/model-specific engineering growth criterion이 아니라 의도적으로 넓은 numerical bound입니다.

동일한 954개 interior face 전체 centroid-skewness 평가에서는 maximum normalized offset `0.20174085968313984`가 관찰됐습니다. 이 metric은 두 owner-cell centroid를 잇는 선과 face plane의 교점을 구하고, 그 교점과 face centroid 사이 거리를 face RMS vertex radius로 정규화합니다. Desktop의 `1e12` maximum과 20,000,000-interior-face budget은 engineering skewness criterion이 아니라 넓은 numerical ownership bound입니다.

이 contract들은 단순 proximity 기반 correspondence나 일반적인 tetrahedral validity보다 강한 evidence이지만, 여전히 다음을 **입증하지 않습니다**: analytic/CAD surface identity, CAD feature topology, source tessellation과 독립적인 continuous curvature, 정확한 source/output edge identity, layered boundary-layer mesh, controlled boundary-layer growth, wall-model/y+ suitability, 범용 solver-specific engineering mesh-quality threshold, engineering CFD accuracy.

따라서 현재 상태는 다음과 같습니다.

- external TetGen case는 `unclassified_audited_volume` 상태 유지;
- `body_fitted_status`는 `not_established` 상태 유지;
- `engineering_quality_status`는 `not_established` 상태 유지;
- `Su2MeshFidelity`에는 의도적으로 아직 `BodyFitted` variant가 없습니다.

Engineering aerodynamic claim에는 신뢰할 수 있는 dimensional reference 비교와 독립적인 grid/domain/model sensitivity 또는 convergence evidence가 필요합니다. TetGen/SU2가 성공적으로 실행됐다는 사실만으로는 이러한 evidence가 되지 않습니다.

## 문서

- `docs/ARCHITECTURE.md` — editor/solver architecture 및 ownership model;
- `docs/VALIDATION.md` — numerical evidence ledger 및 historical validation checkpoint;
- `docs/TETGEN_EXTERNAL_BACKEND.md` — 현재 external TetGen process 및 evidence contract;
- `docs/EXTERIOR_MESHER_ADMISSION.md` — source admission 및 geometry gate;
- `docs/EXTERIOR_MESHER_HANDOFF.md` — solver-bound ownership/evidence hierarchy;
- `docs/MESH_FIDELITY.md` — persisted fidelity state 및 명시적으로 보장하지 않는 항목.

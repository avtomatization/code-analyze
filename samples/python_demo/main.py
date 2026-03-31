from app.service.project_coordinator import ProjectCoordinator


def main() -> None:
    coordinator = ProjectCoordinator()
    coordinator.run("u-1001", 89.50)


if __name__ == "__main__":
    main()

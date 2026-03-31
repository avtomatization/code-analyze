class LoggerUtil:
    @staticmethod
    def info(message: str) -> None:
        print(f"[INFO] {message}")

    @staticmethod
    def error(message: str) -> None:
        print(f"[ERROR] {message}")

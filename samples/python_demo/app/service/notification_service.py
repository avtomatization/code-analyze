from app.service.email_gateway import EmailGateway
from app.util.logger_util import LoggerUtil


class NotificationService:
    def __init__(self) -> None:
        self._email_gateway = EmailGateway()

    def send_payment_status(self, user_id: str, paid: bool) -> None:
        status = "PAID" if paid else "FAILED"
        LoggerUtil.info(f"Sending payment status {status} for {user_id}")
        self._email_gateway.deliver(user_id, f"Payment status: {status}")

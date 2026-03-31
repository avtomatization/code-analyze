from app.service.billing_service import BillingService
from app.service.notification_service import NotificationService


class ProjectCoordinator:
    def __init__(self) -> None:
        self._billing_service = BillingService()
        self._notification_service = NotificationService()

    def run(self, user_id: str, amount: float) -> None:
        paid = self._billing_service.charge_user(user_id, amount)
        self._notification_service.send_payment_status(user_id, paid)

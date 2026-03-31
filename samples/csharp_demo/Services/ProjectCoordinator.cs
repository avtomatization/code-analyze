namespace Demo.Services;

public class ProjectCoordinator
{
    private readonly InvoiceService _invoiceService;
    private readonly NotificationService _notificationService;

    public ProjectCoordinator()
    {
        _invoiceService = new InvoiceService();
        _notificationService = new NotificationService();
    }

    public void Run(string userId, decimal amount)
    {
        var paid = _invoiceService.ChargeUser(userId, amount);
        _notificationService.SendPaymentStatus(userId, paid);
    }
}

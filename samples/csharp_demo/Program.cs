using Demo.Services;

namespace Demo;

public class Program
{
    public static void Main(string[] args)
    {
        var coordinator = new ProjectCoordinator();
        coordinator.Run("u-1001", 89.50m);
    }
}

import net.rsprot.crypto.cipher.IsaacRandom;

public class IsaacOracle {
    public static void main(String[] args) {
        int[] seed = new int[args.length];
        for (int i = 0; i < args.length; i++) seed[i] = (int) Long.parseLong(args[i]);
        IsaacRandom random = new IsaacRandom(seed);
        for (int i = 0; i < 600; i++) {
            if (i != 0) System.out.print(" ");
            System.out.print(Integer.toUnsignedLong(random.nextInt()));
        }
        System.out.println();
    }
}

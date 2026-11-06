import java.nio.file.Files;
import java.nio.file.Paths;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;

public class AccountState {
    public static void main(String[] args) throws Exception {
        if (args.length != 1 || !args[0].matches("[a-z0-9_]{1,12}")) {
            throw new IllegalArgumentException("one probe username required");
        }
        String port = Files.readAllLines(Paths.get("/world/.data/postgres/postmaster.pid")).get(3).trim();
        if (!port.matches("[0-9]+")) throw new IllegalStateException("invalid PostgreSQL port");
        try (Connection db = DriverManager.getConnection("jdbc:postgresql://127.0.0.1:" + port + "/postgres", "postgres", "")) {
            db.setReadOnly(true);
            String query = "SELECT a.id,c.id,a.rights,p.x,p.z,p.level,p.xp_rate_in_hundreds "
                + "FROM public.accounts a JOIN public.account_characters c ON c.account_id=a.id "
                + "JOIN main.character_progress p ON p.character_id=c.id "
                + "WHERE LOWER(a.account_name)=LOWER(?)";
            try (PreparedStatement sql = db.prepareStatement(query)) {
                sql.setString(1, args[0]);
                try (ResultSet row = sql.executeQuery()) {
                    if (!row.next()) throw new IllegalStateException("no persisted character");
                    System.out.println("account=" + row.getLong(1) + " character=" + row.getLong(2)
                        + " rights=" + row.getString(3) + " x=" + row.getInt(4) + " z=" + row.getInt(5)
                        + " level=" + row.getInt(6) + " xp_rate_in_hundreds=" + row.getInt(7));
                    long character = row.getLong(2);
                    try (PreparedStatement stats = db.prepareStatement("SELECT stat_id,base_level,vis_level,fine_xp FROM main.stats WHERE character_id=? ORDER BY stat_id")) {
                        stats.setLong(1, character);
                        try (ResultSet values = stats.executeQuery()) {
                            while (values.next()) System.out.println("stat=" + values.getString(1) + " base=" + values.getInt(2)
                                + " visible=" + values.getInt(3) + " fine_xp=" + values.getLong(4));
                        }
                    }
                    try (PreparedStatement inv = db.prepareStatement("SELECT inv,obj,SUM(count) FROM main.inventory_objs WHERE character_id=? GROUP BY inv,obj ORDER BY inv,obj")) {
                        inv.setLong(1, character);
                        try (ResultSet values = inv.executeQuery()) {
                            while (values.next()) System.out.println("inv=" + values.getString(1) + " item=" + values.getString(2) + " count=" + values.getLong(3));
                        }
                    }
                    if (row.next()) throw new IllegalStateException("ambiguous probe character");
                }
            }
        }
    }
}

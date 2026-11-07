import java.nio.file.Files;
import java.nio.file.Paths;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.Statement;

public class Baseline {
    public static void main(String[] args) throws Exception {
        String port = Files.readAllLines(Paths.get("/world/.data/postgres/postmaster.pid")).get(3).trim();
        if (!port.matches("[0-9]+")) throw new IllegalStateException("invalid PostgreSQL port");
        try (Connection db = DriverManager.getConnection("jdbc:postgresql://127.0.0.1:" + port + "/postgres", "postgres", "")) {
            db.setAutoCommit(false);
            try (Statement sql = db.createStatement()) {
                int changed = sql.executeUpdate("UPDATE realms SET player_xp_rate_in_hundreds=100, global_xp_rate_in_hundreds=100, dev_mode=0 WHERE realm_id=255");
                if (changed != 1) throw new IllegalStateException("expected the isolated realm 255");
                try (ResultSet rows = sql.executeQuery("SELECT player_xp_rate_in_hundreds, global_xp_rate_in_hundreds, dev_mode FROM realms WHERE realm_id=255")) {
                    if (!rows.next() || rows.getInt(1) != 100 || rows.getInt(2) != 100 || rows.getBoolean(3)) {
                        throw new IllegalStateException("baseline verification failed");
                    }
                }
            }
            db.commit();
        }
        System.out.println("Baseline persisted: 1x base XP, 1x global XP, automatic admin disabled. Restart the world to activate.");
    }
}

import { Module } from "@nestjs/common";
import { ConfigModule } from "@nestjs/config";
import { ErrorService } from "@/utils/error.util";

@Module({
    imports: [ConfigModule],
    providers: [ErrorService],
    exports: [ErrorService],
})
export class UtilModule {}

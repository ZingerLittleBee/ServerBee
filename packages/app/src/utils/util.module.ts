import {Module} from "@nestjs/common";
import {JwtUtilService} from "@/utils/jwt.util.service";

@Module({
    imports: [],
    providers: [JwtUtilService],
    exports: [JwtUtilService]
})
export class UtilModule{}

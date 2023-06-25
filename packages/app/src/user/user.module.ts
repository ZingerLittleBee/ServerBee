import { Module } from '@nestjs/common';
import {DbModule} from "@/db/db.module";
import {UserService} from "@/user/user.service";
import { UserController } from './user.controller';

@Module({
    imports: [DbModule],
    controllers: [UserController],
    providers: [UserService],
    exports: [UserService],
})
export class UserModule {}

import { Module } from '@nestjs/common';
import { ProfileService } from './profile.service';
import { ProfileController } from './profile.controller';
import {DbModule} from "@/db/db.module";
import {UserModule} from "@/user/user.module";
import {JwtModule} from "@nestjs/jwt";

@Module({
  imports: [JwtModule.register({}), DbModule, UserModule],
  controllers: [ProfileController],
  providers: [ProfileService]
})
export class ProfileModule {}

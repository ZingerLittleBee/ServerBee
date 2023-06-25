import { Module } from '@nestjs/common';
import {PrismaService} from "./prisma.service";

@Module({
    imports: [],
    controllers: [],
    providers: [PrismaService],
    exports: [PrismaService],
})
export class DbModule {}

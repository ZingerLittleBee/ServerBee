import { Injectable } from '@nestjs/common';
import { User, Prisma } from '@prisma/client';
import {PrismaService} from "@/db/prisma.service";
import {CreateUserDto} from "@/user/dto/create-user.dto";
import {UserVo} from "@/user/vo/user.vo";

@Injectable()
export class UserService {
    constructor(private prisma: PrismaService) {}

    async user(
        userWhereUniqueInput: Prisma.UserWhereUniqueInput,
    ): Promise<User | null> {
        return this.prisma.user.findUnique({
            where: userWhereUniqueInput,
        });
    }

    async getOneByUserId(userId: string) {
        const val =  await this.prisma.user.findUnique({
            where: {
                user_id: userId
            }
        })
        return {
            userId: userId,
            username: val?.username,
            email: val?.email
        } as UserVo
    }

    async getAllUsers(): Promise<User[]> {
        return this.prisma.user.findMany();
    }

    async createUser(data: CreateUserDto): Promise<User> {
        let user: Prisma.UserCreateInput = {
            email: data.email,
            username: data.username,
            password: data.password,
        }
        return this.prisma.user.create({
            data: user,
        });
    }

    async updateUser(params: {
        where: Prisma.UserWhereUniqueInput;
        data: Prisma.UserUpdateInput;
    }): Promise<User> {
        const { where, data } = params;
        return this.prisma.user.update({
            data,
            where,
        });
    }

    async deleteUser(where: Prisma.UserWhereUniqueInput): Promise<User> {
        return this.prisma.user.delete({
            where,
        });
    }
}
